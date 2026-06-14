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
use crate::routes::run_blocking;
use crate::space::users::{normalise_email, UsersRegistry};
use crate::space::Space;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "hearth_session";

/// Smallest passphrase accepted on `/auth/init`, re-checked server-side so a
/// CLI bypass can't register a trivially crackable space. New registrations
/// only; existing spaces keep their original passphrase.
const MIN_PASSPHRASE_LEN: usize = 12;

/// Upper bounds on accepted JSON-body strings, clamped early so an oversized
/// field never reaches `normalise_email`, scrypt, or `.users.toml`.
const MAX_EMAIL_LEN: usize = 254;
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

/// TLS is terminated by the reverse proxy, so HTTPS is detected from the
/// proxy's `X-Forwarded-Proto` header rather than the socket.
fn request_is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
}

/// Build the session cookie. No `Max-Age`/`Expires`, so it survives reloads but
/// clears when the browser closes; the server-side store enforces the real
/// lifetime. `Secure` is set per request — only when the operator opted in and
/// the request arrived over HTTPS — because browsers drop a `Secure` cookie
/// over plain HTTP, which would log dev users out on refresh.
fn session_cookie(
    state: &AppState,
    headers: &HeaderMap,
    value: impl Into<String>,
) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, value.into());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_path("/");
    cookie.set_secure(state.config.cookie_secure && request_is_https(headers));
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

/// Translate a per-IP rate-limit hit into `429 + Retry-After`. Shared by
/// `/auth/init`, `/auth/unlock`, and `/auth/passkey/info` so an attacker can't
/// sidestep the limiter by alternating endpoints.
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
    /// At least one user is registered; drives the SPA's registration-vs-login
    /// choice.
    any_users: bool,
    unlocked: bool,
    owner: String,
    email: String,
    has_passkey: bool,
}

impl StatusResponse {
    fn locked(any_users: bool) -> Self {
        Self {
            any_users,
            unlocked: false,
            owner: String::new(),
            email: String::new(),
            has_passkey: false,
        }
    }
}

async fn status(State(state): State<AppState>, jar: CookieJar) -> Json<StatusResponse> {
    let any_users = state.any_users();
    let session = jar
        .get(SESSION_COOKIE)
        .and_then(|c| Uuid::parse_str(c.value()).ok())
        .and_then(|id| state.sessions.get(&id));

    let Some(session) = session else {
        return Json(StatusResponse::locked(any_users));
    };
    let Ok(space) = state.space_for(&session.user_uuid) else {
        return Json(StatusResponse::locked(any_users));
    };

    let email = state
        .users_snapshot()
        .users
        .iter()
        .find(|u| u.uuid == session.user_uuid)
        .map(|u| u.email.clone())
        .unwrap_or_default();
    let cfg = space.config();
    Json(StatusResponse {
        any_users,
        unlocked: true,
        owner: cfg.owner.clone(),
        email,
        has_passkey: cfg.passkey.is_some(),
    })
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
    user_uuid: String,
}

/// Registration: mints a UUID, creates `<root>/<uuid>/.space.toml` + git-backed
/// `space/`, records the `email -> uuid` mapping, then mints a session cookie.
///
/// Registers the `email -> uuid` mapping before touching disk so an email
/// collision surfaces as a `400` early. If space initialisation then fails
/// mid-way, the registry entry (and any partial directory) is rolled back so the
/// email isn't permanently locked out.
async fn init(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<InitRequest>,
) -> AppResult<impl IntoResponse> {
    enforce_throttle(&state, remote)?;

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

    let entry = state.register_user(&email)?;
    let space_dir = UsersRegistry::space_dir_for(&state.root, &entry.uuid);

    let space_dir_for_init = space_dir.clone();
    let owner_for_init = owner.clone();
    let passphrase_for_init = passphrase.clone();
    let init_result = run_blocking(move || {
        crate::space::init::init_space(crate::space::init::InitOptions {
            space_dir: space_dir_for_init,
            passphrase: age::secrecy::SecretString::from(passphrase_for_init),
            owner: owner_for_init,
        })
    })
    .await;

    if let Err(e) = init_result {
        state.unregister_user(&entry.uuid);
        let _ = std::fs::remove_dir_all(&space_dir);
        return Err(e);
    }

    let space = Space::open(space_dir)?;
    state.cache_space(entry.uuid, space);

    state.unlock_limiter.clear(remote.ip());

    let id = state
        .sessions
        .create(age::secrecy::SecretString::from(passphrase), entry.uuid);
    let jar = jar.add(session_cookie(&state, &headers, id.to_string()));
    let resp_headers = headers_from_jar(&jar);

    let body = Json(InitResponse {
        user_uuid: entry.uuid.to_string(),
    });
    Ok((StatusCode::CREATED, resp_headers, body))
}

#[derive(Deserialize)]
struct UnlockRequest {
    email: String,
    passphrase: String,
}

/// Verify a passphrase against a registered space and mint a session cookie.
///
/// Throttles before the scrypt work so a flood can't pin the worker pool;
/// scrypt is CPU-bound (~150 ms) and runs off the async worker. An unknown
/// email, an oversized field, and a broken space all collapse to
/// `WrongPassphrase` so the response never reveals which addresses are
/// registered.
async fn unlock(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<UnlockRequest>,
) -> AppResult<impl IntoResponse> {
    enforce_throttle(&state, remote)?;

    if req.email.len() > MAX_EMAIL_LEN {
        return Err(AppError::WrongPassphrase);
    }
    if req.passphrase.len() > MAX_PASSPHRASE_LEN {
        return Err(AppError::WrongPassphrase);
    }

    let normalised = normalise_email(&req.email).map_err(|_| AppError::WrongPassphrase)?;
    let entry = state
        .find_user_by_email(&normalised)
        .ok_or(AppError::WrongPassphrase)?;
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

    let passphrase_for_kdf = req.passphrase.clone();
    let log_n = cfg.kdf_log_n;
    let r = cfg.kdf_r;
    let p = cfg.kdf_p;
    let derived =
        run_blocking(move || kdf::derive_verifier(&passphrase_for_kdf, &salt, log_n, r, p)).await?;

    let mut expected_arr = [0u8; kdf::VERIFIER_LEN];
    expected_arr.copy_from_slice(&expected);
    if !kdf::verify(&derived, &expected_arr) {
        return Err(AppError::WrongPassphrase);
    }

    state.unlock_limiter.clear(remote.ip());

    let id = state
        .sessions
        .create(age::secrecy::SecretString::from(req.passphrase), entry.uuid);
    let jar = jar.add(session_cookie(&state, &headers, id.to_string()));
    let resp_headers = headers_from_jar(&jar);
    Ok((StatusCode::NO_CONTENT, resp_headers))
}

/// Drop the session and clear its cookie. The cleared cookie is built through
/// the same helper as the one we set, so its attributes match and the browser
/// actually removes it.
async fn lock(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> impl IntoResponse {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        if let Ok(id) = Uuid::parse_str(c.value()) {
            state.sessions.drop(&id);
        }
    }
    let cookie = session_cookie(&state, &headers, "");
    let jar = jar.remove(cookie);
    let resp_headers = headers_from_jar(&jar);
    (StatusCode::NO_CONTENT, resp_headers)
}

#[derive(Serialize)]
struct PasskeyInfoResponse {
    credential_id_b64: String,
    prf_salt_b64: String,
    wrapped_passphrase_b64: String,
}

#[derive(Deserialize)]
struct PasskeyInfoQuery {
    email: String,
}

/// Public passkey material for `?email=`, needed by the browser to start a
/// WebAuthn assertion *before* the user is authenticated. Unauthenticated by
/// necessity; the throttle bounds enumeration of the known user set, and the
/// wrapped passphrase stays opaque to the server.
async fn passkey_info(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    axum::extract::Query(q): axum::extract::Query<PasskeyInfoQuery>,
) -> AppResult<Json<PasskeyInfoResponse>> {
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

/// Upper bounds on passkey base64 strings so a client can't store megabytes in
/// `.space.toml`, which is read into memory on every request.
const MAX_PASSKEY_CREDENTIAL_LEN: usize = 4 * 1024;
const MAX_PASSKEY_SALT_LEN: usize = 1024;
const MAX_PASSKEY_WRAPPED_LEN: usize = 16 * 1024;

/// Persist passkey wrapping material for the unlocked user.
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

/// Resolve the active session into `(passphrase, Space)` or `Unauthorized`.
/// Shared by every protected handler so they short-circuit identically.
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
