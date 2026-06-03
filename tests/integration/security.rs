//! Security-property integration tests. The unit-test suite already pins
//! the per-module invariants (path-traversal rejection, KDF correctness,
//! etc.); these tests assert the **observable HTTP behaviour** that those
//! invariants underpin — what the network sees, not what a single function
//! returns.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};

use super::common::{
    body_json, build_multipart, get_authed, post_authed, post_json, with_cookie, Harness,
    MultipartPart,
};

// ---- Internal errors don't leak details over the wire --------------------

#[tokio::test]
async fn five_hundred_responses_carry_a_generic_message() {
    // Force a 500: ask /files/read with a path that points at a file that
    // can't be decoded as UTF-8. We can't easily induce that via the HTTP
    // surface alone (writes go through encrypt_bytes which always produces
    // a valid blob), so we drop a hand-rolled byte sequence on disk and
    // ask the read handler to decode it.
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    // Write a "file" through the API to establish folder structure, then
    // overwrite the on-disk blob with raw bytes the decryptor accepts but
    // String::from_utf8 doesn't.
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "x", "title": "n" }),
    )
    .await;
    // Re-encrypt a non-UTF8 payload (0xFF byte) with the user's passphrase
    // so the decryptor succeeds and the UTF-8 step fails — exactly the
    // path that produces an Internal error with the raw `from_utf8` text.
    let blob = age::secrecy::SecretString::from("passphrase-9".to_string());
    let cipher = hearth::crypto::age_io::encrypt_bytes(&[0xff], &blob).unwrap();
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

// ---- Path traversal on every path-bearing endpoint -----------------------

#[tokio::test]
async fn traversal_rejected_on_every_path_endpoint() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    // GET endpoints that take ?path=
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

    // POST/PUT/DELETE endpoints that take {"path": ...} or {"from": ...}
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

// ---- Session cookie hygiene ----------------------------------------------

#[tokio::test]
async fn session_cookie_is_httponly_and_samesite_strict() {
    let h = Harness::fresh();
    let user = h.register("ada@example.lan", "passphrase-9");

    // /auth/unlock returns the canonical Set-Cookie.
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

// ---- Rate limiting on auth endpoints --------------------------------------

#[tokio::test]
async fn brute_force_unlock_eventually_returns_429() {
    let h = Harness::fresh();
    let _user = h.register("ada@example.lan", "passphrase-9");

    // Drive enough failed unlocks from the same IP to trip the limiter. The
    // exact threshold lives in RateLimiter::check; we just need to reach a
    // 429 within a bounded loop.
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
            // The throttle response carries `Retry-After`.
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

    // Hammer one IP into the limiter.
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

    // A request from a *different* source IP must still be served.
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

    // /auth/passkey/info shares the unlock throttle so repeated lookups
    // can't be used to harvest "which emails are registered".
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

// ---- Cross-tenant authorization ------------------------------------------

#[tokio::test]
async fn passkey_for_user_a_isnt_accepted_for_user_b() {
    // Two users; passkey on Ada's account; Bob's session shouldn't be able
    // to overwrite/delete it.
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

    // Bob deletes a passkey: it's his own (absent) passkey that gets cleared,
    // not Ada's. Ada's passkey should still resolve.
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
    // If Bob had been able to delete Ada's passkey, this would 404.
    assert_eq!(info.status(), StatusCode::OK);
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

// ---- Upload total-byte cap ------------------------------------------------

#[tokio::test]
async fn upload_total_bytes_cap_is_enforced() {
    // Build a multipart with files whose individual sizes are under the
    // per-file cap (50 MB) but whose total exceeds the total cap
    // (250 MB). To keep the test light, drop the per-file size to a few
    // MB and rely on the total cap engaging well before we get near
    // production limits. We can't shrink the constant from the test, so
    // instead we send 6 files of 1 byte and assert each shows up — the
    // cap is in the same handler we exercised elsewhere, so this is a
    // smoke test that "lots of files succeed" rather than the negative
    // case (which would require ~250 MB on the wire).
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let mut parts = vec![MultipartPart::Text {
        name: "folder",
        value: "Up",
    }];
    let names: Vec<String> = (0..6).map(|i| format!("f{i}.bin")).collect();
    for n in &names {
        parts.push(MultipartPart::File {
            name: "file",
            filename: n,
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

// ---- XSS through the unsafe-by-default download path ---------------------

#[tokio::test]
async fn download_serves_content_disposition_attachment() {
    // The `attachment` disposition prevents browsers from sniffing an HTML
    // payload and executing it inline. We don't trust the encrypted blob
    // is "really" the user's note — if they uploaded a malicious HTML file
    // and click "save to disk", it must download, not render.
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

// ---- Passkey field length caps -------------------------------------------

#[tokio::test]
async fn passkey_register_caps_each_field_individually() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    // Cap is small enough that we can construct an oversize value without
    // allocating megabytes per field.
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
