//! HTTP integration tests for `/api/auth/*`, driving the live axum router
//! end-to-end (registration runs `init_space` at production KDF cost, unlock
//! runs scrypt, cookies thread through as a browser's would). Tests that don't
//! need that cost use `Harness::register`, which writes a cheap-KDF space.

use axum::body::Body;
use axum::http::header;
use axum::http::Method;
use axum::http::Request;
use axum::http::StatusCode;

use super::common::{
    body_json, expect_status, future_exp, get, mint_sso_token, post_json, urlencode, with_cookie,
    with_sso_cookie, Harness, SESSION_COOKIE,
};

const SSO_SECRET: &str = "integration-sso-shared-secret";
const SSO_SUB: &str = "11111111-1111-4111-8111-111111111111";
const SSO_EMAIL: &str = "ada@drive.example";

/// A POST with a JSON body and an extra `X-Forwarded-Proto: https` header, the
/// way the production reverse proxy presents a TLS request to the app.
fn post_json_https(uri: &str, body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-proto", "https")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn set_cookie_of(res: &axum::http::Response<Body>) -> String {
    res.headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .expect("a Set-Cookie header")
}

#[tokio::test]
async fn init_registers_a_first_user_and_mints_a_session() {
    let h = Harness::fresh();
    let res = h
        .send(post_json_https(
            "/api/auth/init",
            &serde_json::json!({
                "email": "ada@example.lan",
                "passphrase": "correct horse battery",
                "owner": "Ada Lovelace",
            }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let cookie = set_cookie_of(&res);
    assert!(cookie.starts_with(SESSION_COOKIE), "got: {cookie}");
    assert!(cookie.contains("HttpOnly"), "got: {cookie}");
    let body = body_json(res).await;
    assert!(
        body["user_uuid"].as_str().is_some_and(|s| !s.is_empty()),
        "init should return the new uuid: {body}"
    );

    // The freshly registered user can immediately unlock with the same secret.
    let unlock = h
        .send(post_json(
            "/api/auth/unlock",
            &serde_json::json!({ "email": "ada@example.lan", "passphrase": "correct horse battery" }),
        ))
        .await;
    assert_eq!(unlock.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn unlock_over_https_sets_a_secure_cookie_when_enabled() {
    let h = Harness::with_cookie_secure(true);
    let user = h.register("ada@example.lan", "passphrase-9");
    let res = h
        .send(post_json_https(
            "/api/auth/unlock",
            &serde_json::json!({ "email": user.email, "passphrase": "passphrase-9" }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let cookie = set_cookie_of(&res);
    assert!(
        cookie.contains("Secure"),
        "expected Secure over https: {cookie}"
    );
}

#[tokio::test]
async fn unlock_without_https_leaves_cookie_insecure_even_when_enabled() {
    let h = Harness::with_cookie_secure(true);
    let user = h.register("ada@example.lan", "passphrase-9");
    let res = h
        .send(post_json(
            "/api/auth/unlock",
            &serde_json::json!({ "email": user.email, "passphrase": "passphrase-9" }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let cookie = set_cookie_of(&res);
    assert!(
        !cookie.contains("Secure"),
        "plain HTTP must not mark the cookie Secure, or dev refreshes log out: {cookie}"
    );
}

#[tokio::test]
async fn sso_reports_signed_out_without_a_trusted_cookie() {
    let h = Harness::fresh();

    // No SSO cookie at all -> signed out.
    let res = h.send(get("/api/auth/sso")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["signed_in"], serde_json::json!(false));
    assert!(body["email"].is_null());
    assert!(body["sub"].is_null());

    // A present-but-unverifiable cookie still reports signed out: tests configure
    // no shared secret, so no token can be trusted. This also exercises the
    // cookie-present branch of the handler.
    let mut req = get("/api/auth/sso");
    req.headers_mut().insert(
        header::COOKIE,
        "drive_sso=not-a-real-token".parse().unwrap(),
    );
    let res = h.send(req).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["signed_in"], serde_json::json!(false));
}

#[tokio::test]
async fn sso_space_provision_then_unlock_round_trip() {
    let h = Harness::with_sso(SSO_SECRET);
    let token = mint_sso_token(SSO_SECRET, SSO_SUB, SSO_EMAIL, future_exp());

    // No space yet for this drive user.
    let res = h
        .send(with_sso_cookie(get("/api/auth/sso/space"), &token))
        .await;
    let body = body_json(expect_status(res, StatusCode::OK)).await;
    assert_eq!(body["exists"], serde_json::json!(false));
    assert!(body["passkey"].is_null());

    // Provision the encrypted space with a random passphrase + passkey material.
    let provision = serde_json::json!({
        "passphrase": "provisioned-random-passphrase",
        "credential_id_b64": "Y3JlZGVudGlhbA",
        "prf_salt_b64": "c2FsdHk",
        "wrapped_passphrase_b64": "d3JhcHBlZA",
    });
    let res = h
        .send(with_sso_cookie(
            post_json("/api/auth/sso/provision", &provision),
            &token,
        ))
        .await;
    let res = expect_status(res, StatusCode::CREATED);
    assert!(
        res.headers().get(header::SET_COOKIE).is_some(),
        "provision should mint a session cookie"
    );

    // The space now exists and returns its passkey-unlock material.
    let res = h
        .send(with_sso_cookie(get("/api/auth/sso/space"), &token))
        .await;
    let body = body_json(expect_status(res, StatusCode::OK)).await;
    assert_eq!(body["exists"], serde_json::json!(true));
    assert_eq!(body["passkey"]["credential_id_b64"], "Y3JlZGVudGlhbA");

    // A second provision is refused — the client should unlock instead.
    let res = h
        .send(with_sso_cookie(
            post_json("/api/auth/sso/provision", &provision),
            &token,
        ))
        .await;
    expect_status(res, StatusCode::BAD_REQUEST);

    // Unlock with the correct passphrase succeeds.
    let res = h
        .send(with_sso_cookie(
            post_json(
                "/api/auth/sso/unlock",
                &serde_json::json!({ "passphrase": "provisioned-random-passphrase" }),
            ),
            &token,
        ))
        .await;
    expect_status(res, StatusCode::NO_CONTENT);

    // A wrong passphrase, and an oversize one, are both rejected.
    let oversize = "x".repeat(2000);
    for bad in ["the-wrong-passphrase", oversize.as_str()] {
        let res = h
            .send(with_sso_cookie(
                post_json(
                    "/api/auth/sso/unlock",
                    &serde_json::json!({ "passphrase": bad }),
                ),
                &token,
            ))
            .await;
        expect_status(res, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn sso_endpoints_reject_a_missing_or_bad_token() {
    let h = Harness::with_sso(SSO_SECRET);

    // No SSO cookie at all -> unauthorized.
    let res = h.send(get("/api/auth/sso/space")).await;
    expect_status(res, StatusCode::UNAUTHORIZED);

    // A token whose subject isn't a UUID can't map to a space -> bad request.
    let bad_sub = mint_sso_token(SSO_SECRET, "not-a-uuid", SSO_EMAIL, future_exp());
    let res = h
        .send(with_sso_cookie(get("/api/auth/sso/space"), &bad_sub))
        .await;
    expect_status(res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sso_unlock_without_a_space_is_unauthorized() {
    let h = Harness::with_sso(SSO_SECRET);
    let token = mint_sso_token(SSO_SECRET, SSO_SUB, SSO_EMAIL, future_exp());
    let res = h
        .send(with_sso_cookie(
            post_json(
                "/api/auth/sso/unlock",
                &serde_json::json!({ "passphrase": "anything-at-all-here" }),
            ),
            &token,
        ))
        .await;
    expect_status(res, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sso_provision_validates_field_bounds() {
    let h = Harness::with_sso(SSO_SECRET);
    let token = mint_sso_token(SSO_SECRET, SSO_SUB, SSO_EMAIL, future_exp());
    let cases = [
        // passphrase too short (< 12 chars)
        serde_json::json!({ "passphrase": "short", "credential_id_b64": "a", "prf_salt_b64": "a", "wrapped_passphrase_b64": "a" }),
        // passphrase too long
        serde_json::json!({ "passphrase": "x".repeat(2000), "credential_id_b64": "a", "prf_salt_b64": "a", "wrapped_passphrase_b64": "a" }),
        // credential id too long
        serde_json::json!({ "passphrase": "a-fine-passphrase", "credential_id_b64": "x".repeat(5000), "prf_salt_b64": "a", "wrapped_passphrase_b64": "a" }),
        // prf salt too long
        serde_json::json!({ "passphrase": "a-fine-passphrase", "credential_id_b64": "a", "prf_salt_b64": "x".repeat(2000), "wrapped_passphrase_b64": "a" }),
        // wrapped passphrase too long
        serde_json::json!({ "passphrase": "a-fine-passphrase", "credential_id_b64": "a", "prf_salt_b64": "a", "wrapped_passphrase_b64": "x".repeat(20000) }),
    ];
    for body in cases {
        let res = h
            .send(with_sso_cookie(
                post_json("/api/auth/sso/provision", &body),
                &token,
            ))
            .await;
        expect_status(res, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn init_rejects_oversize_passphrase() {
    let h = Harness::fresh();
    let res = h
        .send(post_json(
            "/api/auth/init",
            &serde_json::json!({ "email": "ada@example.lan", "passphrase": "p".repeat(2000) }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn init_rejects_oversize_owner() {
    let h = Harness::fresh();
    let res = h
        .send(post_json(
            "/api/auth/init",
            &serde_json::json!({
                "email": "ada@example.lan",
                "passphrase": "passphrase-9",
                "owner": "o".repeat(300),
            }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unlock_rejects_oversize_fields_as_wrong_passphrase() {
    let h = Harness::fresh();
    let _user = h.register("ada@example.lan", "passphrase-9");
    for body in [
        serde_json::json!({ "email": format!("{}@x.lan", "a".repeat(300)), "passphrase": "passphrase-9" }),
        serde_json::json!({ "email": "ada@example.lan", "passphrase": "p".repeat(2000) }),
    ] {
        let res = h.send(post_json("/api/auth/unlock", &body)).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "body: {body}");
    }
}

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

    let status = body_json(h.send(with_cookie(get("/api/auth/status"), &user)).await).await;
    assert_eq!(status["has_passkey"], true);

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
