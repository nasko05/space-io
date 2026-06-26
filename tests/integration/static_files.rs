//! HTTP integration tests for the embedded SPA bundle served as the router
//! fallback. They prove the `rust-embed` wiring resolves real assets, serves
//! `index.html` at the root, and falls back to it for unknown client routes so
//! deep links survive a hard refresh.

use axum::http::{header, StatusCode};

use super::common::{body_bytes, get, Harness};

#[tokio::test]
async fn root_serves_the_spa_index_html() {
    let h = Harness::fresh();
    let res = h.send(get("/")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "got content-type: {ct}");
    assert!(
        !body_bytes(res).await.is_empty(),
        "index.html should have a body"
    );
}

#[tokio::test]
async fn unknown_path_falls_back_to_index_for_client_routing() {
    let h = Harness::fresh();
    let res = h.send(get("/some/deep/spa/link")).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(!body_bytes(res).await.is_empty());
}

#[tokio::test]
async fn a_named_asset_is_served_directly() {
    let h = Harness::fresh();
    let res = h.send(get("/index.html")).await;
    assert_eq!(res.status(), StatusCode::OK);
}
