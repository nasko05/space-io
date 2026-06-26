//! Persistence-across-restart tests: the property the deploy docs promise —
//! "user data is never lost on redeploy". A redeploy is, from the app's point
//! of view, a process restart over the same data dir (the named volume / host
//! bind mount is reused). These drive the real HTTP surface, tear the server
//! down with `Harness::reopen`, and assert the data is still there afterwards.

use axum::http::StatusCode;

use super::common::{
    body_json, get, get_authed, post_authed, put_authed, urlencode, with_cookie, Harness,
};

#[tokio::test]
async fn a_note_written_before_a_restart_is_intact_after() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    // Write a note through the real create + write handlers (encrypted blob on
    // disk, committed to the per-user git repo).
    let created = post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "Journal", "title": "Persisted" }),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let path = body_json(created).await["path"]
        .as_str()
        .expect("create returns a path")
        .to_string();

    let body_text = "# Persisted\n\nthis must survive a redeploy";
    let wrote = put_authed(
        &h,
        &u,
        "/api/files/write",
        &serde_json::json!({ "path": path, "content": body_text }),
    )
    .await;
    assert_eq!(wrote.status(), StatusCode::OK);

    // Redeploy: drop the running state (incl. the in-memory session + space
    // cache) and boot fresh over the same data dir.
    let h = h.reopen();

    // The pre-restart session is gone, exactly as after a real restart, so the
    // stale cookie no longer authenticates.
    let stale = h.send(with_cookie(get("/api/files/tree"), &u)).await;
    assert_eq!(
        stale.status(),
        StatusCode::UNAUTHORIZED,
        "the old session must not survive a restart"
    );

    // But the registry persisted: the same passphrase unlocks the same space.
    let u = h.reunlock("ada@example.lan", "passphrase-9").await;

    // And the note is byte-for-byte intact — decrypted from the blob the
    // previous process wrote.
    let read = get_authed(
        &h,
        &u,
        &format!("/api/files/read?path={}", urlencode(&path)),
    )
    .await;
    assert_eq!(read.status(), StatusCode::OK);
    assert_eq!(
        body_json(read).await["content"],
        body_text,
        "note content must survive the restart unchanged"
    );
}

#[tokio::test]
async fn the_user_registry_persists_and_blocks_re_registration_after_restart() {
    let h = Harness::fresh();
    let _ = h.register("ada@example.lan", "passphrase-9");

    // Before the restart a user exists.
    let before = body_json(h.send(get("/api/auth/status")).await).await;
    assert_eq!(before["any_users"], true);

    let h = h.reopen();

    // The freshly booted process loaded `.users.toml` from disk: still one user.
    let after = body_json(h.send(get("/api/auth/status")).await).await;
    assert_eq!(
        after["any_users"], true,
        "the registry must reload from disk on restart"
    );

    // Proof the mapping (not just the count) survived: re-registering the same
    // email is rejected as a duplicate rather than silently minting a new,
    // empty space that would shadow the persisted one.
    let dup = h
        .send(super::common::post_json(
            "/api/auth/init",
            &serde_json::json!({
                "email": "ada@example.lan",
                "passphrase": "passphrase-9",
            }),
        ))
        .await;
    assert_eq!(
        dup.status(),
        StatusCode::BAD_REQUEST,
        "an already-registered email must stay registered across a restart"
    );
}
