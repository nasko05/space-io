//! HTTP integration tests for `/api/agent/*`.
//!
//! Run with no provider key, so they cover the non-network wiring: session
//! enforcement, "unconfigured" reporting, and request validation. The agent
//! loop itself is covered by the unit tests under `src/agent/`.

use axum::http::StatusCode;

use super::common::{body_json, get, get_authed, post_authed, post_json, Harness};

#[tokio::test]
async fn status_requires_a_session() {
    let h = Harness::fresh();
    let res = h.send(get("/api/agent/status")).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn status_reports_unconfigured_without_a_key() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let res = get_authed(&h, &u, "/api/agent/status").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    assert_eq!(body["configured"], false);
    assert!(
        body["model"].as_str().is_some_and(|m| !m.is_empty()),
        "model id should be reported: {body}"
    );
    let web = body["web_search"].as_str().unwrap();
    assert!(
        matches!(web, "builtin" | "brave" | "off"),
        "unexpected web_search: {web}"
    );
}

#[tokio::test]
async fn chat_requires_a_session() {
    let h = Harness::fresh();
    let res = h
        .send(post_json(
            "/api/agent/chat",
            &serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }] }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn chat_rejects_empty_messages() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let res = post_authed(
        &h,
        &u,
        "/api/agent/chat",
        &serde_json::json!({ "messages": [] }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn chat_rejects_a_trailing_assistant_message() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let res = post_authed(
        &h,
        &u,
        "/api/agent/chat",
        &serde_json::json!({ "messages": [{ "role": "assistant", "content": "stray" }] }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_rejects_too_many_messages() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let msgs: Vec<_> = (0..201)
        .map(|_| serde_json::json!({ "role": "user", "content": "hi" }))
        .collect();
    let res = post_authed(
        &h,
        &u,
        "/api/agent/chat",
        &serde_json::json!({ "messages": msgs }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(res).await["error"]["code"], "bad_request");
}

#[tokio::test]
async fn chat_rejects_an_oversize_conversation() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let big = "x".repeat(600_001);
    let res = post_authed(
        &h,
        &u,
        "/api/agent/chat",
        &serde_json::json!({ "messages": [{ "role": "user", "content": big }] }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_is_unconfigured_without_a_key() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let res = post_authed(
        &h,
        &u,
        "/api/agent/chat",
        &serde_json::json!({ "messages": [{ "role": "user", "content": "hello" }] }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    let msg = body["error"]["message"].as_str().unwrap();
    assert!(msg.contains("not configured"), "got: {msg}");
}
