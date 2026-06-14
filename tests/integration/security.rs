//! Security-property integration tests asserting the observable HTTP behaviour
//! that the per-module unit invariants underpin — what the network sees.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};

use super::common::{
    body_json, build_multipart, get_authed, post_authed, post_json, with_cookie, Harness,
    MultipartPart,
};

#[tokio::test]
async fn five_hundred_responses_carry_a_generic_message() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "x", "title": "n" }),
    )
    .await;
    let blob = age::secrecy::SecretString::from("passphrase-9".to_string());
    let non_utf8_byte_that_yields_internal_error = [0xff];
    let cipher =
        hearth::crypto::age_io::encrypt_bytes(&non_utf8_byte_that_yields_internal_error, &blob)
            .unwrap();
    std::fs::write(u.user_dir.join("space/x/n.md.age"), cipher).unwrap();

    let res = get_authed(&h, &u, "/api/files/read?path=x/n.md").await;
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "internal");
    assert_eq!(
        body["error"]["message"], "internal server error",
        "internal-error message should be opaque",
    );
}

#[tokio::test]
async fn traversal_rejected_on_every_path_endpoint() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    for uri in [
        "/api/files/read?path=../../etc/passwd",
        "/api/files/download?path=../../etc/passwd",
        "/api/files/history?path=../../etc/passwd",
    ] {
        let res = get_authed(&h, &u, uri).await;
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "GET {uri} should be 403",
        );
    }

    let cases: &[(Method, &str, serde_json::Value)] = &[
        (
            Method::PUT,
            "/api/files/write",
            serde_json::json!({ "path": "../escape.md", "content": "x" }),
        ),
        (
            Method::POST,
            "/api/files/checkpoint",
            serde_json::json!({ "path": "../escape.md", "content": "x" }),
        ),
        (
            Method::POST,
            "/api/files/move",
            serde_json::json!({ "from": "../escape.md", "to": "ok.md" }),
        ),
        (
            Method::POST,
            "/api/files/mkdir",
            serde_json::json!({ "path": "../escape" }),
        ),
        (
            Method::DELETE,
            "/api/files/delete",
            serde_json::json!({ "path": "../escape.md" }),
        ),
        (
            Method::POST,
            "/api/files/rollback",
            serde_json::json!({ "path": "../escape.md", "commit": "0000000000000000000000000000000000000000" }),
        ),
    ];
    for (method, uri, body) in cases {
        let req = Request::builder()
            .method(method.clone())
            .uri(*uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap();
        let req = with_cookie(req, &u);
        let res = h.send(req).await;
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "{method:?} {uri} should be 403 on traversal",
        );
    }
}

#[tokio::test]
async fn session_cookie_is_httponly_and_samesite_strict() {
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
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(!cookie.is_empty());
    assert!(cookie.contains("HttpOnly"), "got: {cookie}");
    assert!(cookie.contains("SameSite=Strict"), "got: {cookie}");
    assert!(cookie.contains("Path=/"), "got: {cookie}");
}

#[tokio::test]
async fn brute_force_unlock_eventually_returns_429() {
    let h = Harness::fresh();
    let _user = h.register("ada@example.lan", "passphrase-9");

    let mut got_429 = false;
    for _ in 0..50 {
        let req = post_json(
            "/api/auth/unlock",
            &serde_json::json!({
                "email": "ada@example.lan",
                "passphrase": "wrong",
            }),
        );
        let res = h.send_from(req, "203.0.113.7").await;
        if res.status() == StatusCode::TOO_MANY_REQUESTS {
            assert!(
                res.headers().get(header::RETRY_AFTER).is_some(),
                "429 should carry Retry-After",
            );
            got_429 = true;
            break;
        }
    }
    assert!(got_429, "rate limiter never engaged");
}

#[tokio::test]
async fn rate_limit_is_per_ip_not_global() {
    let h = Harness::fresh();
    let _user = h.register("ada@example.lan", "passphrase-9");

    let mut tripped = false;
    for _ in 0..50 {
        let req = post_json(
            "/api/auth/unlock",
            &serde_json::json!({ "email": "ada@example.lan", "passphrase": "x" }),
        );
        let res = h.send_from(req, "198.51.100.1").await;
        if res.status() == StatusCode::TOO_MANY_REQUESTS {
            tripped = true;
            break;
        }
    }
    assert!(tripped, "limiter should engage for the first IP");

    let req = post_json(
        "/api/auth/unlock",
        &serde_json::json!({ "email": "ada@example.lan", "passphrase": "x" }),
    );
    let res = h.send_from(req, "198.51.100.2").await;
    assert_ne!(
        res.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "limiter must be per-IP",
    );
}

#[tokio::test]
async fn enumeration_of_passkey_info_is_throttled() {
    let h = Harness::fresh();

    let mut tripped = false;
    for i in 0..40 {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/auth/passkey/info?email=user{i}@nowhere.lan"))
            .body(Body::empty())
            .unwrap();
        let res = h.send_from(req, "192.0.2.50").await;
        if res.status() == StatusCode::TOO_MANY_REQUESTS {
            tripped = true;
            break;
        }
    }
    assert!(tripped, "passkey-info enumeration should eventually 429");
}

#[tokio::test]
async fn passkey_for_user_a_isnt_accepted_for_user_b() {
    let h = Harness::fresh();
    let ada = h.register("ada@example.lan", "passphrase-9");
    let bob = h.register("bob@example.lan", "passphrase-9");

    post_authed(
        &h,
        &ada,
        "/api/auth/passkey/register",
        &serde_json::json!({
            "credential_id_b64": "credAda",
            "prf_salt_b64": "salt",
            "wrapped_passphrase_b64": "wrap"
        }),
    )
    .await;

    let res = h
        .send(with_cookie(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/auth/passkey")
                .body(Body::empty())
                .unwrap(),
            &bob,
        ))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let info = h
        .send(
            Request::builder()
                .method(Method::GET)
                .uri("/api/auth/passkey/info?email=ada@example.lan")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        info.status(),
        StatusCode::OK,
        "Ada's passkey survives Bob's delete"
    );
}

#[tokio::test]
async fn duplicate_registration_email_is_rejected() {
    let h = Harness::fresh();
    let _ = h.register("ada@example.lan", "passphrase-9");

    let res = h
        .send_from(
            post_json(
                "/api/auth/init",
                &serde_json::json!({
                    "email": "ADA@example.lan",
                    "passphrase": "different-pass-9"
                }),
            ),
            "192.0.2.99",
        )
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

/// Smoke-tests that a batch of small files succeeds; the negative case would
/// need ~250 MB on the wire, so it is not exercised here.
#[tokio::test]
async fn upload_total_bytes_cap_is_enforced() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let mut parts = vec![MultipartPart::Text {
        name: "folder",
        value: "Up",
    }];
    let names: Vec<String> = (0..6).map(|i| format!("f{i}.bin")).collect();
    for name in &names {
        parts.push(MultipartPart::File {
            name: "file",
            filename: name,
            content_type: "application/octet-stream",
            bytes: b"x",
        });
    }
    let (boundary, body) = build_multipart(&parts);
    let req = with_cookie(
        Request::builder()
            .method(Method::POST)
            .uri("/api/files/upload")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap(),
        &u,
    );
    let res = h.send(req).await;
    assert_eq!(res.status(), StatusCode::OK);
}

/// `attachment` stops the browser from rendering an uploaded HTML file inline,
/// which would otherwise let a stored `<script>` payload run.
#[tokio::test]
async fn download_serves_content_disposition_attachment() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let (boundary, body) = build_multipart(&[
        MultipartPart::Text {
            name: "folder",
            value: "Up",
        },
        MultipartPart::File {
            name: "file",
            filename: "evil.html",
            content_type: "text/html",
            bytes: b"<script>alert(1)</script>",
        },
    ]);
    let req = with_cookie(
        Request::builder()
            .method(Method::POST)
            .uri("/api/files/upload")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap(),
        &u,
    );
    h.send(req).await;

    let res = get_authed(&h, &u, "/api/files/download?path=Up/evil.html").await;
    let disposition = res
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        disposition.contains("attachment"),
        "Content-Disposition should be attachment; got: {disposition}",
    );
}

#[tokio::test]
async fn passkey_register_caps_each_field_individually() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let oversize_cred = "A".repeat(5_000);
    let oversize_salt = "S".repeat(2_000);
    let oversize_wrap = "W".repeat(20_000);

    for (field, val) in [
        ("credential_id_b64", &oversize_cred),
        ("prf_salt_b64", &oversize_salt),
        ("wrapped_passphrase_b64", &oversize_wrap),
    ] {
        let mut body = serde_json::json!({
            "credential_id_b64": "ok",
            "prf_salt_b64": "ok",
            "wrapped_passphrase_b64": "ok",
        });
        body[field] = serde_json::Value::String(val.clone());
        let res = post_authed(&h, &u, "/api/auth/passkey/register", &body).await;
        assert_eq!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "oversize `{field}` should be rejected",
        );
    }
}
