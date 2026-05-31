use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::PasskeyConfig;
use crate::crypto::kdf;
use crate::error::{AppError, AppResult};
use crate::space::users::{normalise_email, UsersRegistry};
use crate::space::Space;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "hearth_session";

/// Smallest passphrase we'll accept on `/auth/init`. The frontend enforces the
/// same minimum, but the backend re-checks it so a curl/CLI bypass can't sneak
/// in a trivially crackable space. Applies to new registrations only; existing
/// spaces keep whatever they were created with.
const MIN_PASSPHRASE_LEN: usize = 12;

/// Upper bounds on the JSON-body strings we accept. axum's `Json` extractor
/// already caps the body at ~2 MB, but a 2 MB email passed `normalise_email`
/// and was then written verbatim into `.users.toml`; clamp the inputs early.
const MAX_EMAIL_LEN: usize = 254; // RFC 5321 mailbox cap
const MAX_PASSPHRASE_LEN: usize = 1024;
const MAX_OWNER_LEN: usize = 200;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/status", get(status))
        .route("/auth/init", post(init))
        .route("/auth/unlock", post(unlock))
        .route("/auth/lock", post(lock))
        .route("/auth/passkey/info", get(passkey_info))
        .route("/auth/passkey/register", post(passkey_register))
        .route("/auth/passkey", delete(passkey_delete))
}

fn session_cookie(state: &AppState, value: impl Into<String>) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, value.into());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_path("/");
    cookie.set_secure(state.config.cookie_secure);
    cookie
}

fn headers_from_jar(jar: &CookieJar) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for c in jar.iter() {
        if let Ok(val) = HeaderValue::from_str(&c.to_string()) {
            headers.append(header::SET_COOKIE, val);
        }
    }
    headers
}

/// Translate a rate-limit hit into the `429 + Retry-After` response. Both
/// `/auth/init` and `/auth/unlock` use the same per-IP throttle so an
/// attacker can't sidestep the limiter by alternating endpoints.
fn enforce_throttle(state: &AppState, remote: SocketAddr) -> AppResult<()> {
    if let Some(retry_after) = state.unlock_limiter.check(remote.ip()) {
        return Err(AppError::TooManyRequests {
            retry_after_secs: retry_after.as_secs().max(1),
        });
    }
    Ok(())
}

#[derive(Serialize)]
struct StatusResponse {
    /// At least one user is registered. Drives the SPA's choice between the
    /// registration page (false) and the login page (true).
    any_users: bool,
    /// Current cookie maps to a live session.
    unlocked: bool,
    /// Owner display name of the unlocked user; empty when locked.
    owner: String,
    /// Email of the unlocked user; empty when locked.
    email: String,
    /// Whether the unlocked user has a registered passkey.
    has_passkey: bool,
}

async fn status(State(state): State<AppState>, jar: CookieJar) -> Json<StatusResponse> {
    let any_users = state.any_users();
    let session = jar
        .get(SESSION_COOKIE)
        .and_then(|c| Uuid::parse_str(c.value()).ok())
        .and_then(|id| state.sessions.get(&id));

    match session {
        Some(s) => {
            // Resolve the user's email from the registry (cheaper than holding
            // it on the Session struct, and lets the email survive renames).
            let users = state.users_snapshot();
            let email = users
                .users
                .iter()
                .find(|u| u.uuid == s.user_uuid)
                .map(|u| u.email.clone())
                .unwrap_or_default();
            match state.space_for(&s.user_uuid) {
                Ok(space) => {
                    let cfg = space.config();
                    Json(StatusResponse {
                        any_users,
                        unlocked: true,
                        owner: cfg.owner.clone(),
                        email,
                        has_passkey: cfg.passkey.is_some(),
                    })
                }
                Err(_) => Json(StatusResponse {
                    any_users,
                    unlocked: false,
                    owner: String::new(),
                    email: String::new(),
                    has_passkey: false,
                }),
            }
        }
        None => Json(StatusResponse {
            any_users,
            unlocked: false,
            owner: String::new(),
            email: String::new(),
            has_passkey: false,
        }),
    }
}

#[derive(Deserialize)]
struct InitRequest {
    email: String,
    passphrase: String,
    /// Optional display name. Defaults to the email if omitted.
    #[serde(default)]
    owner: Option<String>,
}

#[derive(Serialize)]
struct InitResponse {
    /// The UUID assigned to this user. Surfaced to the SPA so users can see
    /// the folder name maps cleanly to an opaque identifier (and to make the
    /// "mapped it" half of the request explicit).
    user_uuid: String,
}

/// First-run (or additional-user) registration. Mints a UUID, creates
/// `<root>/<uuid>/.space.toml` + git-backed `space/`, appends the
/// `email -> uuid` mapping to `<root>/.users.toml`, then auto-mints a session
/// cookie so the browser drops straight into the Reader.
async fn init(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Json(req): Json<InitRequest>,
) -> AppResult<impl IntoResponse> {
    // Same per-IP throttle as /auth/unlock. Registration is even more
    // expensive (scrypt + git init + initial commit), so a flood is more
    // damaging.
    enforce_throttle(&state, remote)?;

    // Length caps up-front so a multi-megabyte field never makes it into
    // .users.toml or scrypt.
    if req.email.len() > MAX_EMAIL_LEN {
        return Err(AppError::BadRequest("email is too long".into()));
    }
    if req.passphrase.len() > MAX_PASSPHRASE_LEN {
        return Err(AppError::BadRequest("passphrase is too long".into()));
    }
    if req.passphrase.trim().is_empty() {
        return Err(AppError::BadRequest("passphrase must not be empty".into()));
    }
    if req.passphrase.chars().count() < MIN_PASSPHRASE_LEN {
        return Err(AppError::BadRequest(format!(
            "passphrase must be at least {MIN_PASSPHRASE_LEN} characters"
        )));
    }
    if let Some(o) = req.owner.as_ref() {
        if o.len() > MAX_OWNER_LEN {
            return Err(AppError::BadRequest("display name is too long".into()));
        }
    }

    let passphrase = req.passphrase;
    let email = normalise_email(&req.email)?;
    let owner = req
        .owner
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .unwrap_or_else(|| email.clone());

    // Append to the registry first; collisions surface as 400 before we touch
    // the filesystem. Cheap (small TOML write) and atomic enough.
    let entry = state.register_user(&email)?;
    let space_dir = UsersRegistry::space_dir_for(&state.root, &entry.uuid);

    // init_space is CPU-heavy (scrypt + git2). Offload so we don't tie up the
    // tokio reactor.
    let space_dir_for_init = space_dir.clone();
    let owner_for_init = owner.clone();
    let passphrase_for_init = passphrase.clone();
    let init_result = tokio::task::spawn_blocking(move || {
        crate::space::init::init_space(crate::space::init::InitOptions {
            space_dir: space_dir_for_init,
            passphrase: age::secrecy::SecretString::from(passphrase_for_init),
            owner: owner_for_init,
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("init join: {e}")))?;

    // If init_space failed mid-way (disk full, bad permissions, etc.), the
    // registry has an entry pointing at a directory that doesn't exist.
    // Roll back so the user isn't permanently locked out of their email.
    if let Err(e) = init_result {
        state.unregister_user(&entry.uuid);
        // Best-effort cleanup of any partial directory the init left behind.
        let _ = std::fs::remove_dir_all(&space_dir);
        return Err(e);
    }

    let space = Space::open(space_dir)?;
    state.cache_space(entry.uuid, space);

    // Successful sign-up clears the throttle on this IP so the just-created
    // user isn't held back by their own registration attempt.
    state.unlock_limiter.clear(remote.ip());

    let id = state
        .sessions
        .create(age::secrecy::SecretString::from(passphrase), entry.uuid);
    let jar = jar.add(session_cookie(&state, id.to_string()));
    let headers = headers_from_jar(&jar);

    let body = Json(InitResponse {
        user_uuid: entry.uuid.to_string(),
    });
    Ok((StatusCode::CREATED, headers, body))
}

#[derive(Deserialize)]
struct UnlockRequest {
    email: String,
    passphrase: String,
}

async fn unlock(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Json(req): Json<UnlockRequest>,
) -> AppResult<impl IntoResponse> {
    // Throttle brute force per source IP. We count attempts *before* doing
    // the scrypt work so a flood doesn't pin the worker pool.
    enforce_throttle(&state, remote)?;

    if req.email.len() > MAX_EMAIL_LEN {
        return Err(AppError::WrongPassphrase);
    }
    if req.passphrase.len() > MAX_PASSPHRASE_LEN {
        return Err(AppError::WrongPassphrase);
    }

    // Email lookup. Unknown email maps to the same response as a wrong
    // passphrase to avoid leaking which addresses are registered.
    let normalised = normalise_email(&req.email).map_err(|_| AppError::WrongPassphrase)?;
    let entry = state
        .find_user_by_email(&normalised)
        .ok_or(AppError::WrongPassphrase)?;

    // Anything past the email lookup also collapses to "wrong passphrase":
    // a registered email whose space directory is broken (NotFound) still
    // shouldn't tell the attacker that the address exists.
    let space = state.space_for(&entry.uuid).map_err(|e| match e {
        AppError::NotFound => AppError::WrongPassphrase,
        other => other,
    })?;
    let cfg = space.config();
    let salt = hex::decode(&cfg.salt_verify_hex)
        .map_err(|_| AppError::Internal("bad salt hex in config".into()))?;
    let expected = hex::decode(&cfg.verifier_hash_hex)
        .map_err(|_| AppError::Internal("bad verifier hex in config".into()))?;
    if expected.len() != kdf::VERIFIER_LEN {
        return Err(AppError::Internal("verifier length mismatch".into()));
    }

    // scrypt is CPU-bound (~150 ms at our default params) — running it on
    // the async worker would block every other request on the same thread.
    let passphrase_for_kdf = req.passphrase.clone();
    let log_n = cfg.kdf_log_n;
    let r = cfg.kdf_r;
    let p = cfg.kdf_p;
    let derived = tokio::task::spawn_blocking(move || {
        kdf::derive_verifier(&passphrase_for_kdf, &salt, log_n, r, p)
    })
    .await
    .map_err(|e| AppError::Internal(format!("scrypt join: {e}")))??;

    let mut expected_arr = [0u8; kdf::VERIFIER_LEN];
    expected_arr.copy_from_slice(&expected);
    if !kdf::verify(&derived, &expected_arr) {
        return Err(AppError::WrongPassphrase);
    }

    // A successful unlock clears the throttling so a typo earlier in the
    // window doesn't penalise the next legitimate sign-in.
    state.unlock_limiter.clear(remote.ip());

    let id = state
        .sessions
        .create(age::secrecy::SecretString::from(req.passphrase), entry.uuid);
    let jar = jar.add(session_cookie(&state, id.to_string()));
    let headers = headers_from_jar(&jar);
    Ok((StatusCode::NO_CONTENT, headers))
}

async fn lock(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        if let Ok(id) = Uuid::parse_str(c.value()) {
            state.sessions.drop(&id);
        }
    }
    let cookie = session_cookie(&state, "");
    let jar = jar.remove(cookie);
    let headers = headers_from_jar(&jar);
    (StatusCode::NO_CONTENT, headers)
}

#[derive(Serialize)]
struct PasskeyInfoResponse {
    credential_id_b64: String,
    prf_salt_b64: String,
    wrapped_passphrase_b64: String,
}

/// Returns the registered passkey's public material for the user identified
/// by `?email=`. Requires NO session — it's what the browser needs to drive
/// a WebAuthn authentication. The wrapped passphrase is opaque to the server;
/// only the passkey holder can decrypt it.
#[derive(Deserialize)]
struct PasskeyInfoQuery {
    email: String,
}

async fn passkey_info(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    axum::extract::Query(q): axum::extract::Query<PasskeyInfoQuery>,
) -> AppResult<Json<PasskeyInfoResponse>> {
    // Per-IP throttle, same bucket as unlock. The endpoint is unauthenticated
    // and its 200/404 split leaks whether a given email has a passkey. That
    // split is inherent to the pre-auth WebAuthn flow — the browser needs the
    // credential id + PRF salt to start an assertion *before* the user is
    // authenticated, so we can't hide it without breaking passkey login.
    // Accepted risk for the self-hosted / trusted-network deployment model;
    // the per-IP throttle bounds how fast the small, known user set can be
    // enumerated. The wrapped passphrase remains opaque to the server.
    enforce_throttle(&state, remote)?;

    let normalised = normalise_email(&q.email).map_err(|_| AppError::NotFound)?;
    let entry = state
        .find_user_by_email(&normalised)
        .ok_or(AppError::NotFound)?;
    let space = state.space_for(&entry.uuid)?;
    let cfg = space.config();
    let pk = cfg.passkey.ok_or(AppError::NotFound)?;
    Ok(Json(PasskeyInfoResponse {
        credential_id_b64: pk.credential_id_b64,
        prf_salt_b64: pk.prf_salt_b64,
        wrapped_passphrase_b64: pk.wrapped_passphrase_b64,
    }))
}

#[derive(Deserialize)]
struct RegisterPasskeyRequest {
    credential_id_b64: String,
    prf_salt_b64: String,
    wrapped_passphrase_b64: String,
}

/// Generous upper bounds on the base64 strings we'll accept for a passkey
/// registration. Real-world WebAuthn credentials are well under these caps;
/// the limits exist so a malicious client can't store megabytes in the
/// per-user `.space.toml` (which we read into memory on every request).
const MAX_PASSKEY_CREDENTIAL_LEN: usize = 4 * 1024;
const MAX_PASSKEY_SALT_LEN: usize = 1024;
const MAX_PASSKEY_WRAPPED_LEN: usize = 16 * 1024;

/// Persist the passkey wrapping material for the unlocked user. Requires an
/// active session.
async fn passkey_register(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<RegisterPasskeyRequest>,
) -> AppResult<StatusCode> {
    let (_, space) = require_session(&state, &jar)?;
    if req.credential_id_b64.len() > MAX_PASSKEY_CREDENTIAL_LEN {
        return Err(AppError::BadRequest("credential id too long".into()));
    }
    if req.prf_salt_b64.len() > MAX_PASSKEY_SALT_LEN {
        return Err(AppError::BadRequest("prf salt too long".into()));
    }
    if req.wrapped_passphrase_b64.len() > MAX_PASSKEY_WRAPPED_LEN {
        return Err(AppError::BadRequest("wrapped passphrase too long".into()));
    }
    space.set_passkey(Some(PasskeyConfig {
        credential_id_b64: req.credential_id_b64,
        prf_salt_b64: req.prf_salt_b64,
        wrapped_passphrase_b64: req.wrapped_passphrase_b64,
    }))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn passkey_delete(State(state): State<AppState>, jar: CookieJar) -> AppResult<StatusCode> {
    let (_, space) = require_session(&state, &jar)?;
    space.set_passkey(None)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resolve the active session into (passphrase, Space) or return Unauthorized.
/// Centralised so every protected handler short-circuits identically and
/// the Space-cache hits go through one place.
pub fn require_session(
    state: &AppState,
    jar: &CookieJar,
) -> AppResult<(age::secrecy::SecretString, Space)> {
    let id = jar
        .get(SESSION_COOKIE)
        .and_then(|c| Uuid::parse_str(c.value()).ok())
        .ok_or(AppError::Unauthorized)?;
    let session = state.sessions.get(&id).ok_or(AppError::Unauthorized)?;
    let space = state.space_for(&session.user_uuid)?;
    Ok((session.passphrase, space))
}
