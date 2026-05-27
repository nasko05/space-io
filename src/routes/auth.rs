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
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "hearth_session";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/status", get(status))
        .route("/auth/unlock", post(unlock))
        .route("/auth/lock", post(lock))
        .route("/auth/passkey/info", get(passkey_info))
        .route("/auth/passkey/register", post(passkey_register))
        .route("/auth/passkey", delete(passkey_delete))
}

#[derive(Serialize)]
struct StatusResponse {
    unlocked: bool,
    owner: String,
    has_passkey: bool,
}

async fn status(State(state): State<AppState>, jar: CookieJar) -> Json<StatusResponse> {
    let unlocked = jar
        .get(SESSION_COOKIE)
        .and_then(|c| Uuid::parse_str(c.value()).ok())
        .map(|id| state.sessions.get(&id).is_some())
        .unwrap_or(false);
    let cfg = state.space.config();
    Json(StatusResponse {
        unlocked,
        owner: cfg.owner.clone(),
        has_passkey: cfg.passkey.is_some(),
    })
}

#[derive(Deserialize)]
struct UnlockRequest {
    passphrase: String,
}

async fn unlock(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<UnlockRequest>,
) -> AppResult<impl IntoResponse> {
    let cfg = state.space.config();
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
        .create(age::secrecy::SecretString::from(req.passphrase));

    let mut cookie = Cookie::new(SESSION_COOKIE, id.to_string());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_path("/");
    let jar = jar.add(cookie);

    let mut headers = HeaderMap::new();
    for c in jar.iter() {
        if let Ok(val) = HeaderValue::from_str(&c.to_string()) {
            headers.append(header::SET_COOKIE, val);
        }
    }
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
    let mut headers = HeaderMap::new();
    for c in jar.iter() {
        if let Ok(val) = HeaderValue::from_str(&c.to_string()) {
            headers.append(header::SET_COOKIE, val);
        }
    }
    (StatusCode::NO_CONTENT, headers)
}

#[derive(Serialize)]
struct PasskeyInfoResponse {
    credential_id_b64: String,
    prf_salt_b64: String,
    wrapped_passphrase_b64: String,
}

/// Returns the registered passkey's public material. Requires NO auth — it's
/// what the browser needs to drive a WebAuthn authentication. The wrapped
/// passphrase is opaque to the server; only the passkey holder can decrypt it.
async fn passkey_info(State(state): State<AppState>) -> AppResult<Json<PasskeyInfoResponse>> {
    let cfg = state.space.config();
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

/// Persist the passkey wrapping material. Requires an active session so
/// only an already-unlocked user can register a new passkey for this space.
async fn passkey_register(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<RegisterPasskeyRequest>,
) -> AppResult<StatusCode> {
    require_passphrase(&state, &jar)?;
    state.space.set_passkey(Some(PasskeyConfig {
        credential_id_b64: req.credential_id_b64,
        prf_salt_b64: req.prf_salt_b64,
        wrapped_passphrase_b64: req.wrapped_passphrase_b64,
    }))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn passkey_delete(State(state): State<AppState>, jar: CookieJar) -> AppResult<StatusCode> {
    require_passphrase(&state, &jar)?;
    state.space.set_passkey(None)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resolve the active session's passphrase or return Unauthorized.
pub fn require_passphrase(
    state: &AppState,
    jar: &CookieJar,
) -> AppResult<age::secrecy::SecretString> {
    let id = jar
        .get(SESSION_COOKIE)
        .and_then(|c| Uuid::parse_str(c.value()).ok())
        .ok_or(AppError::Unauthorized)?;
    state.sessions.get(&id).ok_or(AppError::Unauthorized)
}
