//! HTTP integration tests for `/api/files/*`, covering the full file CRUD
//! surface: create, read, write, move, delete, mkdir, tree, excerpts, meta
//! tags, history, rollback, and download.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};

use super::common::{
    body_bytes, body_json, build_multipart, delete_authed, get_authed, post_authed, post_json,
    put_authed, urlencode, with_cookie, Harness, MultipartPart,
};

#[tokio::test]
async fn create_file_writes_an_encrypted_blob() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let res = post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "Journal", "title": "Morning notes" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["path"], "Journal/Morning notes.md");

    let on_disk = u.user_dir.join("space/Journal/Morning notes.md.age");
    assert!(on_disk.is_file(), "file should exist on disk");
}

#[tokio::test]
async fn read_then_write_then_read_round_trip() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let r = post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "Notes", "title": "first" }),
    )
    .await;
    let path = body_json(r).await["path"].as_str().unwrap().to_string();

    let r = get_authed(
        &h,
        &u,
        &format!("/api/files/read?path={}", urlencode(&path)),
    )
    .await;
    let body = body_json(r).await;
    assert_eq!(body["content"], "", "a freshly created note is empty");

    let r = put_authed(
        &h,
        &u,
        "/api/files/write",
        &serde_json::json!({
            "path": path,
            "content": "# Hello\n\nbody text",
        }),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);

    let r = get_authed(
        &h,
        &u,
        &format!("/api/files/read?path={}", urlencode(&path)),
    )
    .await;
    let body = body_json(r).await;
    assert_eq!(body["content"], "# Hello\n\nbody text");
    assert!(body["updated"].is_string(), "got: {body:?}");
}

#[tokio::test]
async fn read_rejects_path_traversal() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let res = get_authed(&h, &u, "/api/files/read?path=../../etc/passwd").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn write_rejects_path_traversal() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let res = put_authed(
        &h,
        &u,
        "/api/files/write",
        &serde_json::json!({ "path": "../escape.md", "content": "x" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn read_missing_file_returns_404() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let res = get_authed(&h, &u, "/api/files/read?path=does/not/exist.md").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let h = Harness::fresh();
    let res = h
        .send(post_json(
            "/api/files/create",
            &serde_json::json!({ "folder": "x" }),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tree_lists_created_files() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "A", "title": "one" }),
    )
    .await;
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "B/sub", "title": "two" }),
    )
    .await;

    let res = get_authed(&h, &u, "/api/files/tree").await;
    let body = body_json(res).await;
    let tree = body["tree"].as_array().unwrap();
    let folder_names: Vec<String> = tree
        .iter()
        .filter(|n| n["type"] == "folder")
        .map(|n| n["name"].as_str().unwrap().to_string())
        .collect();
    assert!(folder_names.contains(&"A".to_string()));
    assert!(folder_names.contains(&"B".to_string()));
}

#[tokio::test]
async fn tree_strips_age_suffix_from_filenames() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "A", "title": "note" }),
    )
    .await;

    let body = body_json(get_authed(&h, &u, "/api/files/tree").await).await;
    let folder_a = body["tree"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "A")
        .expect("folder A");
    let file = &folder_a["children"][0];
    assert_eq!(file["type"], "file");
    assert_eq!(file["name"], "note.md");
    assert_eq!(file["path"], "A/note.md");
    assert_eq!(file["kind"], "md");
}

#[tokio::test]
async fn move_renames_the_file_on_disk() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let path = body_json(
        post_authed(
            &h,
            &u,
            "/api/files/create",
            &serde_json::json!({ "folder": "A", "title": "old" }),
        )
        .await,
    )
    .await["path"]
        .as_str()
        .unwrap()
        .to_string();

    let res = post_authed(
        &h,
        &u,
        "/api/files/move",
        &serde_json::json!({ "from": path, "to": "A/new.md" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    assert!(u.user_dir.join("space/A/new.md.age").is_file());
    assert!(!u.user_dir.join("space/A/old.md.age").exists());
}

#[tokio::test]
async fn move_bulk_renames_many_files_in_one_request() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    for n in ["a", "b", "c"] {
        post_authed(
            &h,
            &u,
            "/api/files/create",
            &serde_json::json!({ "folder": "src", "title": n }),
        )
        .await;
    }

    let res = post_authed(
        &h,
        &u,
        "/api/files/move/bulk",
        &serde_json::json!({
            "moves": [
                { "from": "src/a.md", "to": "dst/a.md" },
                { "from": "src/b.md", "to": "dst/b.md" },
                { "from": "src/c.md", "to": "dst/c.md" },
            ]
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);

    for n in ["a", "b", "c"] {
        assert!(
            u.user_dir.join(format!("space/dst/{n}.md.age")).is_file(),
            "moved file exists: {n}",
        );
    }
}

#[tokio::test]
async fn delete_moves_file_to_trash() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let path = body_json(
        post_authed(
            &h,
            &u,
            "/api/files/create",
            &serde_json::json!({ "folder": "A", "title": "doomed" }),
        )
        .await,
    )
    .await["path"]
        .as_str()
        .unwrap()
        .to_string();

    let res = delete_authed(
        &h,
        &u,
        "/api/files/delete",
        &serde_json::json!({ "path": path }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let trash = body["trash_path"].as_str().unwrap();
    assert!(trash.starts_with(".trash/"));
    assert!(trash.ends_with("/A/doomed.md"));

    assert!(!u.user_dir.join("space/A/doomed.md.age").exists());
    assert!(u.user_dir.join(format!("space/{trash}.age")).is_file());
}

#[tokio::test]
async fn delete_bulk_moves_many_files_under_same_timestamp() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    for n in ["a", "b", "c"] {
        post_authed(
            &h,
            &u,
            "/api/files/create",
            &serde_json::json!({ "folder": "x", "title": n }),
        )
        .await;
    }

    let res = delete_authed(
        &h,
        &u,
        "/api/files/delete/bulk",
        &serde_json::json!({
            "paths": ["x/a.md", "x/b.md", "x/c.md"]
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(res).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);

    // Reduce each `.trash/<stamp>/x/a.md` to its `.trash/<stamp>` prefix.
    let prefixes: std::collections::HashSet<&str> = results
        .iter()
        .map(|result| {
            let trash_path = result["trash_path"].as_str().unwrap();
            let mut segments = trash_path.splitn(3, '/');
            let trash_dir = segments.next().unwrap();
            let stamp = segments.next().unwrap();
            Box::leak(format!("{trash_dir}/{stamp}").into_boxed_str()) as &str
        })
        .collect();
    assert_eq!(prefixes.len(), 1, "bulk-delete entries share a timestamp");
}

#[tokio::test]
async fn delete_missing_path_is_404() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let res = delete_authed(
        &h,
        &u,
        "/api/files/delete",
        &serde_json::json!({ "path": "nowhere.md" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn mkdir_creates_an_empty_folder() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let res = post_authed(
        &h,
        &u,
        "/api/files/mkdir",
        &serde_json::json!({ "path": "Plans/2026" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert!(u.user_dir.join("space/Plans/2026").is_dir());
}

#[tokio::test]
async fn mkdir_rejects_traversal() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let res = post_authed(
        &h,
        &u,
        "/api/files/mkdir",
        &serde_json::json!({ "path": "../escape" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn excerpts_returns_title_and_body() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let path = body_json(
        post_authed(
            &h,
            &u,
            "/api/files/create",
            &serde_json::json!({ "folder": "Notes", "title": "n1" }),
        )
        .await,
    )
    .await["path"]
        .as_str()
        .unwrap()
        .to_string();
    put_authed(
        &h,
        &u,
        "/api/files/write",
        &serde_json::json!({
            "path": path,
            "content": "# Real Title\n\nbody line"
        }),
    )
    .await;

    let body = body_json(get_authed(&h, &u, "/api/files/excerpts").await).await;
    let entry = &body["excerpts"]["Notes/n1.md"];
    assert_eq!(entry["title"], "Real Title");
    assert!(
        entry["excerpt"].as_str().unwrap().contains("body line"),
        "got: {entry:?}",
    );
}

#[tokio::test]
async fn set_tags_then_meta_round_trips() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "Notes", "title": "tagged" }),
    )
    .await;

    let res = put_authed(
        &h,
        &u,
        "/api/files/meta",
        &serde_json::json!({
            "path": "Notes/tagged.md",
            "tags": ["one", "two", "  "]   // the blank tag must be dropped
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let body = body_json(get_authed(&h, &u, "/api/files/meta").await).await;
    assert_eq!(
        body["meta"]["Notes/tagged.md"]["tags"],
        serde_json::json!(["one", "two"]),
    );
}

#[tokio::test]
async fn set_tags_bulk_in_one_request() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    for n in ["a", "b", "c"] {
        post_authed(
            &h,
            &u,
            "/api/files/create",
            &serde_json::json!({ "folder": "x", "title": n }),
        )
        .await;
    }
    let res = put_authed(
        &h,
        &u,
        "/api/files/meta/bulk",
        &serde_json::json!({
            "updates": [
                { "path": "x/a.md", "tags": ["first"] },
                { "path": "x/b.md", "tags": ["second"] },
                { "path": "x/c.md", "tags": ["third"] },
            ]
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let body = body_json(get_authed(&h, &u, "/api/files/meta").await).await;
    assert_eq!(body["meta"]["x/a.md"]["tags"], serde_json::json!(["first"]));
    assert_eq!(
        body["meta"]["x/b.md"]["tags"],
        serde_json::json!(["second"])
    );
    assert_eq!(body["meta"]["x/c.md"]["tags"], serde_json::json!(["third"]));
}

#[tokio::test]
async fn empty_tags_removes_the_entry() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "x", "title": "n" }),
    )
    .await;
    put_authed(
        &h,
        &u,
        "/api/files/meta",
        &serde_json::json!({ "path": "x/n.md", "tags": ["t"] }),
    )
    .await;
    put_authed(
        &h,
        &u,
        "/api/files/meta",
        &serde_json::json!({ "path": "x/n.md", "tags": [] }),
    )
    .await;
    let body = body_json(get_authed(&h, &u, "/api/files/meta").await).await;
    assert!(
        body["meta"].get("x/n.md").is_none(),
        "expected entry removed, got: {body:?}",
    );
}

#[tokio::test]
async fn drafts_do_not_create_history_but_checkpoints_do() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "N", "title": "n" }),
    )
    .await;

    // Autosave drafts persist but must not add history entries.
    put_authed(
        &h,
        &u,
        "/api/files/write",
        &serde_json::json!({ "path": "N/n.md", "content": "draft v2" }),
    )
    .await;
    put_authed(
        &h,
        &u,
        "/api/files/write",
        &serde_json::json!({ "path": "N/n.md", "content": "draft v3" }),
    )
    .await;

    let body = body_json(get_authed(&h, &u, "/api/files/history?path=N/n.md").await).await;
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(
        entries.len(),
        1,
        "drafts should not appear in history: {entries:?}"
    );

    let read = body_json(get_authed(&h, &u, "/api/files/read?path=N/n.md").await).await;
    assert_eq!(read["content"], "draft v3", "reads see the latest draft");

    // An explicit checkpoint mints exactly one history entry.
    let res = post_authed(
        &h,
        &u,
        "/api/files/checkpoint",
        &serde_json::json!({ "path": "N/n.md", "content": "draft v3", "message": "🔖 milestone" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(get_authed(&h, &u, "/api/files/history?path=N/n.md").await).await;
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "got: {entries:?}");
    assert_eq!(
        entries[0]["message"], "🔖 milestone",
        "newest first, user label"
    );
    assert_eq!(entries[0]["author"], "hearth");
}

#[tokio::test]
async fn checkpoint_without_message_uses_a_default_label() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "N", "title": "n" }),
    )
    .await;
    let res = post_authed(
        &h,
        &u,
        "/api/files/checkpoint",
        &serde_json::json!({ "path": "N/n.md", "content": "body" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(get_authed(&h, &u, "/api/files/history?path=N/n.md").await).await;
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["message"], "Edit: N/n.md", "default label");
}

#[tokio::test]
async fn history_rejects_path_traversal() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let res = get_authed(&h, &u, "/api/files/history?path=../etc/passwd").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rollback_restores_an_earlier_version() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "N", "title": "n" }),
    )
    .await;
    post_authed(
        &h,
        &u,
        "/api/files/checkpoint",
        &serde_json::json!({ "path": "N/n.md", "content": "first", "message": "first" }),
    )
    .await;
    post_authed(
        &h,
        &u,
        "/api/files/checkpoint",
        &serde_json::json!({ "path": "N/n.md", "content": "second", "message": "second" }),
    )
    .await;

    // History is newest-first, so the "first" checkpoint is entries[1].
    let history = body_json(get_authed(&h, &u, "/api/files/history?path=N/n.md").await).await;
    let entries = history["entries"].as_array().unwrap();
    let first_commit = entries[1]["commit"].as_str().unwrap().to_string();

    let res = post_authed(
        &h,
        &u,
        "/api/files/rollback",
        &serde_json::json!({ "path": "N/n.md", "commit": first_commit }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    let read = body_json(get_authed(&h, &u, "/api/files/read?path=N/n.md").await).await;
    assert_eq!(read["content"], "first");
}

#[tokio::test]
async fn rollback_preserves_an_uncheckpointed_draft() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "N", "title": "n" }),
    )
    .await;
    post_authed(
        &h,
        &u,
        "/api/files/checkpoint",
        &serde_json::json!({ "path": "N/n.md", "content": "v1", "message": "v1" }),
    )
    .await;
    // An autosave draft the user never checkpointed.
    put_authed(
        &h,
        &u,
        "/api/files/write",
        &serde_json::json!({ "path": "N/n.md", "content": "unsaved draft" }),
    )
    .await;

    let history = body_json(get_authed(&h, &u, "/api/files/history?path=N/n.md").await).await;
    let entries = history["entries"].as_array().unwrap();
    let v1_commit = entries[0]["commit"].as_str().unwrap().to_string();

    // The draft must be snapshotted on rollback, not silently dropped.
    let res = post_authed(
        &h,
        &u,
        "/api/files/rollback",
        &serde_json::json!({ "path": "N/n.md", "commit": v1_commit }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let read = body_json(get_authed(&h, &u, "/api/files/read?path=N/n.md").await).await;
    assert_eq!(read["content"], "v1");

    // The draft is recoverable from the "before restore" checkpoint.
    let history = body_json(get_authed(&h, &u, "/api/files/history?path=N/n.md").await).await;
    let entries = history["entries"].as_array().unwrap();
    let before_restore = entries
        .iter()
        .find(|e| {
            e["message"]
                .as_str()
                .unwrap_or("")
                .contains("before restore")
        })
        .expect("draft should have been checkpointed before restore");
    let draft_commit = before_restore["commit"].as_str().unwrap().to_string();
    post_authed(
        &h,
        &u,
        "/api/files/rollback",
        &serde_json::json!({ "path": "N/n.md", "commit": draft_commit }),
    )
    .await;
    let read = body_json(get_authed(&h, &u, "/api/files/read?path=N/n.md").await).await;
    assert_eq!(read["content"], "unsaved draft");
}

#[tokio::test]
async fn rollback_rejects_invalid_oid() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "N", "title": "n" }),
    )
    .await;
    let res = post_authed(
        &h,
        &u,
        "/api/files/rollback",
        &serde_json::json!({ "path": "N/n.md", "commit": "not-a-hash" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_stores_an_encrypted_blob_and_download_round_trips() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let (boundary, body) = build_multipart(&[
        MultipartPart::Text {
            name: "folder",
            value: "Photos",
        },
        MultipartPart::File {
            name: "file",
            filename: "hello.bin",
            content_type: "application/octet-stream",
            bytes: b"hello, world!",
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
    let res = h.send(req).await;
    assert_eq!(res.status(), StatusCode::OK);

    // And download decrypts back to the original bytes.
    let download = get_authed(
        &h,
        &u,
        &format!("/api/files/download?path={}", urlencode("Photos/hello.bin")),
    )
    .await;
    assert_eq!(download.status(), StatusCode::OK);
    let bytes = body_bytes(download).await;
    assert_eq!(bytes, b"hello, world!");
}

#[tokio::test]
async fn upload_accepts_a_file_larger_than_the_default_body_limit() {
    // Regression: axum's 2 MB per-route default once made the multipart parser
    // reject larger uploads before the handler's own checks ran.
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let payload = vec![0xABu8; 3 * 1024 * 1024]; // over the old 2 MB cap
    let (boundary, body) = build_multipart(&[
        MultipartPart::Text {
            name: "folder",
            value: "Photos",
        },
        MultipartPart::File {
            name: "file",
            filename: "big.bin",
            content_type: "application/octet-stream",
            bytes: &payload,
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
    let res = h.send(req).await;
    assert_eq!(res.status(), StatusCode::OK);
    let json = body_json(res).await;
    assert_eq!(json["files"][0]["size"], payload.len());

    let download = get_authed(
        &h,
        &u,
        &format!("/api/files/download?path={}", urlencode("Photos/big.bin")),
    )
    .await;
    assert_eq!(download.status(), StatusCode::OK);
    let bytes = body_bytes(download).await;
    assert_eq!(bytes, payload);
}

#[tokio::test]
async fn upload_rejects_empty_multipart_body() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    let (boundary, body) = build_multipart(&[]);
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
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_too_many_files_in_one_request_is_rejected() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");

    // More parts than MAX_UPLOADS_PER_REQUEST (64).
    let mut parts: Vec<MultipartPart> = vec![MultipartPart::Text {
        name: "folder",
        value: "Up",
    }];
    let names: Vec<String> = (0..70).map(|i| format!("f{i}.bin")).collect();
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
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn download_rejects_path_traversal() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let res = get_authed(&h, &u, "/api/files/download?path=../../etc/passwd").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn search_finds_a_body_match() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    post_authed(
        &h,
        &u,
        "/api/files/create",
        &serde_json::json!({ "folder": "Notes", "title": "thoughts" }),
    )
    .await;
    put_authed(
        &h,
        &u,
        "/api/files/write",
        &serde_json::json!({
            "path": "Notes/thoughts.md",
            "content": "# t\n\nThe quick brown fox jumps."
        }),
    )
    .await;

    let body = body_json(get_authed(&h, &u, "/api/search?q=brown").await).await;
    let hits = body["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["path"], "Notes/thoughts.md");
}

#[tokio::test]
async fn search_returns_empty_for_blank_query() {
    let h = Harness::fresh();
    let u = h.register("ada@example.lan", "passphrase-9");
    let body = body_json(get_authed(&h, &u, "/api/search?q=").await).await;
    assert!(body["hits"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn one_users_session_cannot_read_anothers_files() {
    let h = Harness::fresh();
    let ada = h.register("ada@example.lan", "passphrase-9");
    let bob = h.register("bob@example.lan", "passphrase-9");

    post_authed(
        &h,
        &ada,
        "/api/files/create",
        &serde_json::json!({ "folder": "Secrets", "title": "passwords" }),
    )
    .await;
    put_authed(
        &h,
        &ada,
        "/api/files/write",
        &serde_json::json!({
            "path": "Secrets/passwords.md",
            "content": "hunter2"
        }),
    )
    .await;

    let body = body_json(get_authed(&h, &bob, "/api/files/tree").await).await;
    let names: Vec<String> = body["tree"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        !names.contains(&"Secrets".to_string()),
        "Bob saw Ada's folder: {names:?}",
    );

    // Ada's file lives in her directory, so Bob's read 404s.
    let res = get_authed(&h, &bob, "/api/files/read?path=Secrets/passwords.md").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
