use axum::extract::State;
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
    jar: CookieJar,
    Json(req): Json<InitRequest>,
) -> AppResult<impl IntoResponse> {
    let passphrase = req.passphrase;
    if passphrase.trim().is_empty() {
        return Err(AppError::BadRequest("passphrase must not be empty".into()));
    }

    // Validate + normalise email up-front. `register_user` validates again,
    // but we want a clean 400 before doing any disk work.
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
    // registry entry is orphaned. We can't transparently roll it back without
    // racing concurrent registrations, so log + leave it — the next attempt
    // for the same email will hit BadRequest and the operator can clean up.
    init_result?;

    let space = Space::open(space_dir)?;
    state.cache_space(entry.uuid, space);

    let id = state
        .sessions
        .create(age::secrecy::SecretString::from(passphrase), entry.uuid);
    let jar = jar.add(session_cookie(id));
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
    jar: CookieJar,
    Json(req): Json<UnlockRequest>,
) -> AppResult<impl IntoResponse> {
    // Email lookup. Unknown email maps to the same response as a wrong
    // passphrase to avoid leaking which addresses are registered.
    let normalised = normalise_email(&req.email).map_err(|_| AppError::WrongPassphrase)?;
    let entry = state
        .find_user_by_email(&normalised)
        .ok_or(AppError::WrongPassphrase)?;

    let space = state.space_for(&entry.uuid)?;
    let cfg = space.config();
    let salt = hex::decode(&cfg.salt_verify_hex)
        .map_err(|_| AppError::Internal("bad salt hex in config".into()))?;
    let expected = hex::decode(&cfg.verifier_hash_hex)
        .map_err(|_| AppError::Internal("bad verifier hex in config".into()))?;
    if expected.len() != kdf::VERIFIER_LEN {
        return Err(AppError::Internal("verifier length mismatch".into()));
    }

    let derived =
        kdf::derive_verifier(&req.passphrase, &salt, cfg.kdf_log_n, cfg.kdf_r, cfg.kdf_p)?;
    let mut expected_arr = [0u8; kdf::VERIFIER_LEN];
    expected_arr.copy_from_slice(&expected);
    if !kdf::verify(&derived, &expected_arr) {
        return Err(AppError::WrongPassphrase);
    }

    let id = state
        .sessions
        .create(age::secrecy::SecretString::from(req.passphrase), entry.uuid);
    let jar = jar.add(session_cookie(id));
    let headers = headers_from_jar(&jar);
    Ok((StatusCode::NO_CONTENT, headers))
}

async fn lock(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        if let Ok(id) = Uuid::parse_str(c.value()) {
            state.sessions.drop(&id);
        }
    }
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
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
    axum::extract::Query(q): axum::extract::Query<PasskeyInfoQuery>,
) -> AppResult<Json<PasskeyInfoResponse>> {
    let normalised = normalise_email(&q.email)?;
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

/// Persist the passkey wrapping material for the unlocked user. Requires an
/// active session.
async fn passkey_register(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<RegisterPasskeyRequest>,
) -> AppResult<StatusCode> {
    let (_, space) = require_session(&state, &jar)?;
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

fn session_cookie(id: Uuid) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, id.to_string());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_path("/");
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
