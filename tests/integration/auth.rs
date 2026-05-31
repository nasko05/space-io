//! HTTP integration tests for `/api/auth/*`.
//!
//! These tests drive the live axum router (built via `routes::build_router`)
//! end-to-end: the registration handler runs `init_space` with production
//! KDF parameters, the unlock handler runs scrypt, and the cookie jar is
//! threaded through the same way a browser would carry it.
//!
//! Tests that don't need the production KDF cost use `Harness::register`
//! which sidesteps `/auth/init` and writes a cheap-KDF space directly.

use axum::http::StatusCode;

use super::common::{
    body_json, expect_status, get, post_json, with_cookie, Harness, SESSION_COOKIE,
};

#[tokio::test]
async fn status_on_fresh_space_reports_no_users_no_session() {
    let h = Harness::fresh();
    let res = h.send(get("/api/auth/status")).await;
    let (status, body) = (res.status(), body_json(res).await);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["any_users"], serde_json::Value::Bool(false));
    assert_eq!(body["unlocked"], serde_json::Value::Bool(false));
    assert_eq!(body["owner"], "");
    assert_eq!(body["email"], "");
    assert_eq!(body["has_passkey"], serde_json::Value::Bool(false));
}

#[tokio::test]
async fn status_reports_unlocked_session() {
    let h = Harness::fresh();
    let user = h.register("ada@example.lan", "passphrase-9");

    let res = h.send(with_cookie(get("/api/auth/status"), &user)).await;
    let body = body_json(res).await;
    assert_eq!(body["any_users"], true);
    assert_eq!(body["unlocked"], true);
    assert_eq!(body["email"], "ada@example.lan");
}

#[tokio::test]
async fn status_with_garbage_cookie_is_not_unlocked() {
    let h = Harness::fresh();
    let _user = h.register("ada@example.lan", "passphrase-9");

    let req = Request::builder()
        .uri("/api/auth/status")
        .header(
            axum::http::header::COOKIE,
            format!("{SESSION_COOKIE}=not-a-uuid"),
        )
        .body(axum::body::Body::empty())
        .unwrap();
    let body = body_json(h.send(req).await).await;
    assert_eq!(body["unlocked"], false);
    // The cookie was unparseable, so we can't tell whose space it pointed at.
    assert_eq!(body["owner"], "");
    assert_eq!(body["email"], "");
}

#[tokio::test]
async fn lock_drops_the_session_cookie() {
    let h = Harness::fresh();
    let user = h.register("ada@example.lan", "passphrase-9");

    let res = h
        .send(with_cookie(
            Request::builder()
                .method(axum::http::Method::POST)
                .uri("/api/auth/lock")
                .body(axum::body::Body::empty())
                .unwrap(),
            &user,
        ))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    // The server replies with a Set-Cookie that clears the value; we just
    // check the session is no longer resolvable on the server side.
    let res = h.send(with_cookie(get("/api/auth/status"), &user)).await;
    assert_eq!(body_json(res).await["unlocked"], false);
}

#[tokio::test]
async fn init_rejects_short_passphrase() {
    let h = Harness::fresh();
    let res = h
        .send(post_json(
            "/api/auth/init",
            &serde_json::json!({
                "email": "ada@example.lan",
                "passphrase": "short",
            }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn init_rejects_blank_passphrase() {
    let h = Harness::fresh();
    let res = h
        .send(post_json(
            "/api/auth/init",
            &serde_json::json!({
                "email": "ada@example.lan",
                "passphrase": "        ",
            }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn init_rejects_oversize_email() {
    let h = Harness::fresh();
    let big = format!("{}@example.lan", "a".repeat(300));
    let res = h
        .send(post_json(
            "/api/auth/init",
            &serde_json::json!({ "email": big, "passphrase": "passphrase-9" }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn init_rejects_malformed_email() {
    let h = Harness::fresh();
    let res = h
        .send(post_json(
            "/api/auth/init",
            &serde_json::json!({ "email": "not-an-email", "passphrase": "passphrase-9" }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unlock_with_wrong_email_returns_401_wrong_passphrase() {
    let h = Harness::fresh();
    let _user = h.register("ada@example.lan", "passphrase-9");

    // Same status code + error code as a real-but-wrong passphrase, so an
    // attacker can't tell which addresses are registered.
    let res = h
        .send(post_json(
            "/api/auth/unlock",
            &serde_json::json!({
                "email": "stranger@nowhere.lan",
                "passphrase": "passphrase-9",
            }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(res).await["error"]["code"], "wrong_passphrase");
}

#[tokio::test]
async fn unlock_with_wrong_passphrase_returns_401() {
    let h = Harness::fresh();
    let user = h.register("ada@example.lan", "passphrase-9");

    let res = h
        .send(post_json(
            "/api/auth/unlock",
            &serde_json::json!({
                "email": user.email,
                "passphrase": "definitely-not-it",
            }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(res).await["error"]["code"], "wrong_passphrase");
}

#[tokio::test]
async fn unlock_with_correct_passphrase_mints_a_session_cookie() {
    let h = Harness::fresh();
    let user = h.register("ada@example.lan", "passphrase-9");

    let res = h
        .send(post_json(
            "/api/auth/unlock",
            &serde_json::json!({
                "email": user.email,
                "passphrase": "passphrase-9",
            }),
        ))
        .await;
    let status = res.status();
    let cookie_header = res
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = cookie_header.expect("Set-Cookie returned");
    assert!(cookie.starts_with(SESSION_COOKIE), "got: {cookie}");
    assert!(cookie.contains("HttpOnly"), "expected HttpOnly: {cookie}");
    assert!(cookie.contains("SameSite=Strict"), "got: {cookie}");
}

#[tokio::test]
async fn passkey_info_for_unknown_email_returns_404() {
    let h = Harness::fresh();
    let res = h
        .send(get("/api/auth/passkey/info?email=nobody@nowhere.lan"))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn passkey_register_requires_a_session() {
    let h = Harness::fresh();
    let res = h
        .send(post_json(
            "/api/auth/passkey/register",
            &serde_json::json!({
                "credential_id_b64": "AAAA",
                "prf_salt_b64": "BBBB",
                "wrapped_passphrase_b64": "CCCC",
            }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn passkey_register_rejects_oversize_strings() {
    let h = Harness::fresh();
    let user = h.register("ada@example.lan", "passphrase-9");

    let big = "A".repeat(20 * 1024);
    let res = h
        .send(with_cookie(
            post_json(
                "/api/auth/passkey/register",
                &serde_json::json!({
                    "credential_id_b64": big,
                    "prf_salt_b64": "BBBB",
                    "wrapped_passphrase_b64": "CCCC",
                }),
            ),
            &user,
        ))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn passkey_register_then_info_round_trips() {
    let h = Harness::fresh();
    let user = h.register("ada@example.lan", "passphrase-9");
    let body = serde_json::json!({
        "credential_id_b64": "credBASE64==",
        "prf_salt_b64": "saltBASE64==",
        "wrapped_passphrase_b64": "wrappedBASE64==",
    });
    let res = expect_status(
        h.send(with_cookie(
            post_json("/api/auth/passkey/register", &body),
            &user,
        ))
        .await,
        StatusCode::NO_CONTENT,
    );
    drop(res);

    let info = h
        .send(get(&format!(
            "/api/auth/passkey/info?email={}",
            urlencode(&user.email)
        )))
        .await;
    assert_eq!(info.status(), StatusCode::OK);
    let info = body_json(info).await;
    assert_eq!(info["credential_id_b64"], "credBASE64==");
    assert_eq!(info["prf_salt_b64"], "saltBASE64==");
    assert_eq!(info["wrapped_passphrase_b64"], "wrappedBASE64==");

    // And it shows in /auth/status now.
    let status = body_json(h.send(with_cookie(get("/api/auth/status"), &user)).await).await;
    assert_eq!(status["has_passkey"], true);

    // Delete clears it.
    let res = h
        .send(with_cookie(
            Request::builder()
                .method(axum::http::Method::DELETE)
                .uri("/api/auth/passkey")
                .body(axum::body::Body::empty())
                .unwrap(),
            &user,
        ))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let status = body_json(h.send(with_cookie(get("/api/auth/status"), &user)).await).await;
    assert_eq!(status["has_passkey"], false);
}

#[tokio::test]
async fn unregistered_endpoints_unauthorized_without_cookie() {
    let h = Harness::fresh();
    let _ = h.register("ada@example.lan", "passphrase-9");
    for uri in [
        "/api/files/tree",
        "/api/files/excerpts",
        "/api/files/meta",
        "/api/search?q=anything",
    ] {
        let res = h.send(get(uri)).await;
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "expected 401 for {uri}",
        );
    }
}

// --- shared imports for tests using `Request::builder()` directly ---
use axum::http::Request;

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            _ => {
                for b in ch.to_string().as_bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}
