//! HTTP integration tests for the `/healthz` liveness probe. It must answer
//! `200 ok` on a fresh server with no users and without any session cookie,
//! since the container HEALTHCHECK, the reverse proxy, and the CD smoke test
//! all hit it before anyone has registered.

use axum::http::StatusCode;

use super::common::{body_bytes, get, Harness};

#[tokio::test]
async fn healthz_returns_ok_without_users_or_session() {
    let h = Harness::fresh();
    let res = h.send(get("/healthz")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_bytes(res).await;
    assert_eq!(body, b"ok");
}

#[tokio::test]
async fn healthz_is_not_nested_under_api() {
    let h = Harness::fresh();
    let res = h.send(get("/api/healthz")).await;
    // `/api/healthz` is not a route; it falls through to the SPA handler, which
    // serves index.html (or the bundle-missing notice in test builds), never
    // the probe's `ok`. This pins the probe to the top level so proxy and
    // HEALTHCHECK configs can rely on `/healthz`.
    let body = body_bytes(res).await;
    assert_ne!(body, b"ok");
}
