//! Shared helpers for the HTTP integration tests under `tests/integration/`.
//!
//! Each test builds a fresh `TempDir`-rooted `AppState` and the same axum router
//! the binary uses, then drives it via `tower::ServiceExt::oneshot` to exercise
//! the full deserialise → handler → response cycle. Users are registered with
//! cheap KDF params so startup doesn't pay the production scrypt cost.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, Response, StatusCode};
use axum::Router;
use rand::RngCore;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

use hearth::config::SpaceConfig;
use hearth::crypto::kdf;
use hearth::routes;
use hearth::space::rate_limit::RateLimiter;
use hearth::space::session::SessionStore;
use hearth::space::Space;
use hearth::state::{AppConfig, AppState};

/// Cheap KDF params for tests (matches `space::test_helpers`).
const TEST_LOG_N: u8 = 4;
const TEST_R: u32 = 8;
const TEST_P: u32 = 1;

pub const SESSION_COOKIE: &str = "hearth_session";

pub struct Harness {
    pub tempdir: TempDir,
    pub state: AppState,
    pub router: Router,
}

impl Harness {
    /// Spin up a fresh server-in-a-process. No users yet — call `register` or
    /// `register_via_http` to create one.
    pub fn fresh() -> Self {
        let tempdir = TempDir::new().expect("tempdir");
        std::fs::create_dir_all(tempdir.path()).expect("mkdir root");
        let state = AppState::new(
            tempdir.path().to_path_buf(),
            SessionStore::new(),
            RateLimiter::new(),
            AppConfig {
                cookie_secure: false,
            },
        )
        .expect("AppState::new");
        let router = routes::build_router(state.clone());
        Self {
            tempdir,
            state,
            router,
        }
    }

    pub fn root(&self) -> &Path {
        self.tempdir.path()
    }

    /// Drive a single request through the router, injecting a fake
    /// `ConnectInfo<SocketAddr>` that `oneshot` would otherwise omit (auth
    /// handlers extract it for rate limiting).
    pub async fn send(&self, mut req: Request<Body>) -> Response<Body> {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(addr));
        self.router
            .clone()
            .oneshot(req)
            .await
            .expect("router::oneshot")
    }

    /// Same as `send` but lets callers pretend the request came from a
    /// specific source IP. Useful for rate-limit tests that need each
    /// request to look like a fresh client.
    pub async fn send_from(&self, mut req: Request<Body>, ip: &str) -> Response<Body> {
        let addr: SocketAddr = format!("{ip}:0").parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(addr));
        self.router
            .clone()
            .oneshot(req)
            .await
            .expect("router::oneshot")
    }

    /// Register a user by hand with cheap KDF params, sidestepping the auth
    /// router's full-cost `init_space`. Returns the session cookie so callers can
    /// attach it to later requests.
    pub fn register(&self, email: &str, passphrase: &str) -> RegisteredUser {
        let entry = self
            .state
            .register_user(email)
            .expect("register in user registry");
        let user_dir = self.root().join(entry.uuid.to_string());
        let space_root = SpaceConfig::space_root(&user_dir);
        std::fs::create_dir_all(&space_root).expect("mkdir space root");

        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        let verifier = kdf::derive_verifier(passphrase, &salt, TEST_LOG_N, TEST_R, TEST_P)
            .expect("derive verifier");
        let cfg = SpaceConfig {
            owner: email.into(),
            salt_verify_hex: hex::encode(salt),
            verifier_hash_hex: hex::encode(verifier),
            kdf_log_n: TEST_LOG_N,
            kdf_r: TEST_R,
            kdf_p: TEST_P,
            passkey: None,
        };
        cfg.save(&user_dir).expect("save space config");

        git2::Repository::init(&space_root).expect("git init");

        let space = Space::open(user_dir.clone()).expect("space open");
        self.state.cache_space(entry.uuid, space);
        let cookie = self.state.sessions.create(
            age::secrecy::SecretString::from(passphrase.to_string()),
            entry.uuid,
        );

        RegisteredUser {
            email: entry.email.clone(),
            cookie: cookie.to_string(),
            user_dir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegisteredUser {
    pub email: String,
    pub cookie: String,
    pub user_dir: PathBuf,
}

impl RegisteredUser {
    /// Build a `Cookie` header value suitable for attaching to a request via
    /// `header(http::header::COOKIE, user.cookie_header())`.
    pub fn cookie_header(&self) -> String {
        format!("{SESSION_COOKIE}={}", self.cookie)
    }
}

pub fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

pub fn post_json(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

pub fn put_json(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

pub fn delete_json(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

/// Attach a session cookie to a request before sending it.
pub fn with_cookie(mut req: Request<Body>, user: &RegisteredUser) -> Request<Body> {
    req.headers_mut().insert(
        header::COOKIE,
        user.cookie_header().parse().expect("valid cookie header"),
    );
    req
}

/// Drain a response body into bytes.
pub async fn body_bytes(res: Response<Body>) -> Vec<u8> {
    to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("read body")
        .to_vec()
}

/// Drain a response body and parse as JSON.
pub async fn body_json(res: Response<Body>) -> Value {
    let bytes = body_bytes(res).await;
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "non-json response body: {e}; body was {:?}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

/// Assert a response status and return the response (for chaining).
pub fn expect_status(res: Response<Body>, expected: StatusCode) -> Response<Body> {
    assert_eq!(res.status(), expected, "unexpected response status");
    res
}

/// Common shorthand for "POST a JSON body to a session-authenticated URL".
pub async fn post_authed(
    harness: &Harness,
    user: &RegisteredUser,
    uri: &str,
    body: &Value,
) -> Response<Body> {
    harness.send(with_cookie(post_json(uri, body), user)).await
}

pub async fn put_authed(
    harness: &Harness,
    user: &RegisteredUser,
    uri: &str,
    body: &Value,
) -> Response<Body> {
    harness.send(with_cookie(put_json(uri, body), user)).await
}

pub async fn delete_authed(
    harness: &Harness,
    user: &RegisteredUser,
    uri: &str,
    body: &Value,
) -> Response<Body> {
    harness
        .send(with_cookie(delete_json(uri, body), user))
        .await
}

pub async fn get_authed(harness: &Harness, user: &RegisteredUser, uri: &str) -> Response<Body> {
    harness.send(with_cookie(get(uri), user)).await
}

/// Build a `multipart/form-data` body for upload tests: a minimal hand-rolled
/// serializer, just enough for axum's `Multipart` extractor.
pub fn build_multipart(parts: &[MultipartPart]) -> (String, Vec<u8>) {
    let boundary = format!("----hearth-test-boundary-{}", rand::random::<u32>());
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match part {
            MultipartPart::Text { name, value } => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(value.as_bytes());
            }
            MultipartPart::File {
                name,
                filename,
                content_type,
                bytes,
            } => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n",
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
                body.extend_from_slice(bytes);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

pub enum MultipartPart<'a> {
    Text {
        name: &'a str,
        value: &'a str,
    },
    File {
        name: &'a str,
        filename: &'a str,
        content_type: &'a str,
        bytes: &'a [u8],
    },
}

/// Minimal percent-encoder for query paths in test URLs, so we don't pull in an
/// extra dependency just to build a request.
pub fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            _ => {
                for byte in ch.to_string().as_bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    out
}
