use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Multipart, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use axum::routing::delete;

use crate::error::{AppError, AppResult};
use crate::routes::auth::require_session;
use crate::routes::run_blocking;
use crate::space::{
    create, delete as delete_mod, download, excerpt, history, meta, mkdir, read, rename, rollback,
    tree, upload, write,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/files/tree", get(get_tree))
        .route("/files/read", get(get_read))
        .route("/files/write", put(put_write))
        .route("/files/checkpoint", post(post_checkpoint))
        .route("/files/create", post(post_create))
        .route("/files/excerpts", get(get_excerpts))
        .route(
            "/files/upload",
            // Lift axum's 2 MB default body limit so our own per-file and
            // per-request caps decide what's too big, not the multipart parser.
            post(post_upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_REQUEST_BYTES)),
        )
        .route("/files/download", get(get_download))
        .route("/files/history", get(get_history))
        .route("/files/rollback", post(post_rollback))
        .route("/files/move", post(post_move))
        .route("/files/move/bulk", post(post_move_bulk))
        .route("/files/delete", delete(delete_file))
        .route("/files/delete/bulk", delete(delete_files_bulk))
        .route("/files/mkdir", post(post_mkdir))
        .route("/files/meta", get(get_meta).put(put_meta))
        .route("/files/meta/bulk", put(put_meta_bulk))
}

#[derive(Serialize)]
struct TreeResponse {
    tree: Vec<tree::TreeNode>,
}

async fn get_tree(State(state): State<AppState>, jar: CookieJar) -> AppResult<Json<TreeResponse>> {
    let (_, space) = require_session(&state, &jar)?;
    let tree = run_blocking(move || tree::build_tree(&space)).await?;
    Ok(Json(TreeResponse { tree }))
}

#[derive(Deserialize)]
struct ReadQuery {
    path: String,
}

#[derive(Serialize)]
struct ReadResponse {
    path: String,
    content: String,
    updated: Option<String>,
}

async fn get_read(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<ReadQuery>,
) -> AppResult<Json<ReadResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let result = run_blocking(move || read::read_file(&space, &pass, &q.path)).await?;
    Ok(Json(ReadResponse {
        path: result.path,
        content: result.content,
        updated: result.updated,
    }))
}

#[derive(Deserialize)]
struct WriteRequest {
    path: String,
    content: String,
}

#[derive(Serialize)]
struct WriteResponse {
    path: String,
    updated: String,
}

/// Autosave: persist editor content to the working tree without a commit. Only
/// an explicit `post_checkpoint` mints a history entry.
async fn put_write(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<WriteRequest>,
) -> AppResult<Json<WriteResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let result =
        run_blocking(move || write::save_draft(&space, &pass, &req.path, &req.content)).await?;
    Ok(Json(WriteResponse {
        path: result.path,
        updated: result.updated,
    }))
}

#[derive(Deserialize)]
struct CheckpointRequest {
    path: String,
    content: String,
    message: Option<String>,
}

/// Persist `content` and record it as a commit, with an optional user label —
/// the deliberate "save a version I can return to" counterpart to autosave.
async fn post_checkpoint(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CheckpointRequest>,
) -> AppResult<Json<WriteResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let message = req
        .message
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(|m| m.to_string());
    let result = run_blocking(move || {
        write::write_file(&space, &pass, &req.path, &req.content, message.as_deref())
    })
    .await?;
    Ok(Json(WriteResponse {
        path: result.path,
        updated: result.updated,
    }))
}

#[derive(Deserialize)]
struct CreateRequest {
    folder: String,
    title: Option<String>,
}

#[derive(Serialize)]
struct CreateResponse {
    path: String,
}

async fn post_create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateRequest>,
) -> AppResult<Json<CreateResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let result =
        run_blocking(move || create::create_file(&space, &pass, &req.folder, req.title.as_deref()))
            .await?;
    Ok(Json(CreateResponse { path: result.path }))
}

#[derive(Serialize)]
struct ExcerptItem {
    title: Option<String>,
    excerpt: String,
}

#[derive(Serialize)]
struct ExcerptsResponse {
    excerpts: std::collections::BTreeMap<String, ExcerptItem>,
}

async fn get_excerpts(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Json<ExcerptsResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let raw = run_blocking(move || excerpt::build_excerpts(&space, &pass)).await?;
    let excerpts = raw
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                ExcerptItem {
                    title: v.title,
                    excerpt: v.excerpt,
                },
            )
        })
        .collect();
    Ok(Json(ExcerptsResponse { excerpts }))
}

#[derive(Serialize)]
struct UploadResult {
    path: String,
    size: u64,
}

#[derive(Serialize)]
struct UploadResponse {
    files: Vec<UploadResult>,
}

const DEFAULT_UPLOAD_FOLDER: &str = "Uploads";
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
/// Per-request total. The handler buffers every part before encrypting, so
/// memory grows with the whole body, not the per-file cap.
const MAX_TOTAL_UPLOAD_BYTES: usize = 250 * 1024 * 1024;
/// Per-batch file count, so one request can't monopolise the blocking pool with
/// per-file crypto + git work.
const MAX_UPLOADS_PER_REQUEST: usize = 64;
/// Body-limit ceiling, kept above `MAX_TOTAL_UPLOAD_BYTES` (plus framing
/// overhead) so the handler's descriptive `400` rejects oversized batches rather
/// than the layer truncating into an opaque multipart parse error.
const MAX_UPLOAD_REQUEST_BYTES: usize = 256 * 1024 * 1024;

async fn post_upload(
    State(state): State<AppState>,
    jar: CookieJar,
    mut multipart: Multipart,
) -> AppResult<Json<UploadResponse>> {
    let (pass, space) = require_session(&state, &jar)?;

    let mut folder: Option<String> = None;
    // Keep the ref-counted `Bytes` rather than copying into a `Vec`.
    let mut files: Vec<(String, Bytes)> = Vec::new();
    let mut total_bytes: usize = 0;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart: {e}")))?
    {
        let name = field.name().map(|s| s.to_string()).unwrap_or_default();
        if name == "folder" {
            folder = Some(
                field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("folder field: {e}")))?,
            );
        } else if name == "file" || name == "files" || name == "files[]" {
            if files.len() >= MAX_UPLOADS_PER_REQUEST {
                return Err(AppError::BadRequest(format!(
                    "too many files in one request (max {MAX_UPLOADS_PER_REQUEST})"
                )));
            }
            let filename = field
                .file_name()
                .map(|s| s.to_string())
                .ok_or_else(|| AppError::BadRequest("file part missing filename".into()))?;
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("read body: {e}")))?;
            if bytes.len() > MAX_UPLOAD_BYTES {
                return Err(AppError::BadRequest(format!(
                    "{filename} exceeds {MAX_UPLOAD_BYTES} bytes"
                )));
            }
            let new_total = total_bytes.saturating_add(bytes.len());
            if new_total > MAX_TOTAL_UPLOAD_BYTES {
                return Err(AppError::BadRequest(format!(
                    "request body exceeds {MAX_TOTAL_UPLOAD_BYTES} bytes"
                )));
            }
            total_bytes = new_total;
            files.push((filename, bytes));
        }
    }
    if files.is_empty() {
        return Err(AppError::BadRequest("no files in multipart body".into()));
    }
    let folder = folder.unwrap_or_else(|| DEFAULT_UPLOAD_FOLDER.to_string());

    let results = run_blocking(move || {
        let mut results = Vec::with_capacity(files.len());
        for (name, bytes) in files {
            let r = upload::store_upload(&space, &pass, &folder, &name, &bytes)?;
            results.push(UploadResult {
                path: r.path,
                size: r.size,
            });
        }
        Ok(results)
    })
    .await?;
    Ok(Json(UploadResponse { files: results }))
}

#[derive(Deserialize)]
struct DownloadQuery {
    path: String,
}

async fn get_download(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<DownloadQuery>,
) -> AppResult<Response> {
    let (pass, space) = require_session(&state, &jar)?;
    let file = run_blocking(move || download::fetch_decrypted(&space, &pass, &q.path)).await?;
    let mime = mime_guess::from_path(&file.path).first_or_octet_stream();
    let base_name = std::path::Path::new(&file.path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&file.path);
    let content_disposition = format!("attachment; filename=\"{}\"", base_name.replace('"', ""));
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime.as_ref().to_string()),
            (header::CONTENT_DISPOSITION, content_disposition),
        ],
        Body::from(file.bytes),
    )
        .into_response())
}

#[derive(Deserialize)]
struct HistoryQuery {
    path: String,
}

#[derive(Serialize)]
struct HistoryEntryDto {
    commit: String,
    message: String,
    author: String,
    when: String,
}

#[derive(Serialize)]
struct HistoryResponse {
    entries: Vec<HistoryEntryDto>,
}

async fn get_history(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<HistoryResponse>> {
    let (_, space) = require_session(&state, &jar)?;
    let entries = run_blocking(move || history::file_history(&space, &q.path))
        .await?
        .into_iter()
        .map(|e| HistoryEntryDto {
            commit: e.commit,
            message: e.message,
            author: e.author,
            when: e.when,
        })
        .collect();
    Ok(Json(HistoryResponse { entries }))
}

#[derive(Deserialize)]
struct RollbackRequest {
    path: String,
    commit: String,
}

#[derive(Serialize)]
struct RollbackResponse {
    path: String,
    updated: String,
}

async fn post_rollback(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<RollbackRequest>,
) -> AppResult<Json<RollbackResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let r =
        run_blocking(move || rollback::rollback_to(&space, &pass, &req.path, &req.commit)).await?;
    Ok(Json(RollbackResponse {
        path: r.path,
        updated: r.updated,
    }))
}

#[derive(Deserialize)]
struct MoveRequest {
    from: String,
    to: String,
}

#[derive(Serialize)]
struct MoveResponse {
    path: String,
    is_directory: bool,
}

impl From<rename::MoveResult> for MoveResponse {
    fn from(result: rename::MoveResult) -> Self {
        Self {
            path: result.path,
            is_directory: result.is_directory,
        }
    }
}

async fn post_move(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<MoveRequest>,
) -> AppResult<Json<MoveResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let result =
        run_blocking(move || rename::rename_path(&space, &pass, &req.from, &req.to)).await?;
    Ok(Json(result.into()))
}

#[derive(Deserialize)]
struct MovePair {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct MoveBulkRequest {
    moves: Vec<MovePair>,
}

#[derive(Serialize)]
struct MoveBulkResponse {
    results: Vec<MoveResponse>,
}

async fn post_move_bulk(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<MoveBulkRequest>,
) -> AppResult<Json<MoveBulkResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let pairs: Vec<(String, String)> = req.moves.into_iter().map(|m| (m.from, m.to)).collect();
    let results = run_blocking(move || rename::rename_paths_bulk(&space, &pass, pairs)).await?;
    Ok(Json(MoveBulkResponse {
        results: results.into_iter().map(MoveResponse::from).collect(),
    }))
}

#[derive(Deserialize)]
struct DeleteRequest {
    path: String,
}

#[derive(Serialize)]
struct DeleteResponse {
    trash_path: String,
}

impl From<delete_mod::DeleteResult> for DeleteResponse {
    fn from(result: delete_mod::DeleteResult) -> Self {
        Self {
            trash_path: result.trash_path,
        }
    }
}

async fn delete_file(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<DeleteRequest>,
) -> AppResult<Json<DeleteResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let result =
        run_blocking(move || delete_mod::delete_to_trash(&space, &pass, &req.path)).await?;
    Ok(Json(result.into()))
}

#[derive(Deserialize)]
struct DeleteBulkRequest {
    paths: Vec<String>,
}

#[derive(Serialize)]
struct DeleteBulkResponse {
    results: Vec<DeleteResponse>,
}

async fn delete_files_bulk(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<DeleteBulkRequest>,
) -> AppResult<Json<DeleteBulkResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let results =
        run_blocking(move || delete_mod::delete_to_trash_bulk(&space, &pass, req.paths)).await?;
    Ok(Json(DeleteBulkResponse {
        results: results.into_iter().map(DeleteResponse::from).collect(),
    }))
}

#[derive(Deserialize)]
struct MkdirRequest {
    path: String,
}

async fn post_mkdir(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<MkdirRequest>,
) -> AppResult<StatusCode> {
    let (_, space) = require_session(&state, &jar)?;
    run_blocking(move || mkdir::create_folder(&space, &req.path)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct MetaItem {
    tags: Vec<String>,
}

#[derive(Serialize)]
struct MetaResponse {
    meta: std::collections::BTreeMap<String, MetaItem>,
}

async fn get_meta(State(state): State<AppState>, jar: CookieJar) -> AppResult<Json<MetaResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let idx = run_blocking(move || meta::load(&space, &pass)).await?;
    let meta = idx
        .paths
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                MetaItem {
                    tags: v.tags.clone(),
                },
            )
        })
        .collect();
    Ok(Json(MetaResponse { meta }))
}

#[derive(Deserialize)]
struct PutMetaRequest {
    path: String,
    tags: Vec<String>,
}

async fn put_meta(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<PutMetaRequest>,
) -> AppResult<StatusCode> {
    let (pass, space) = require_session(&state, &jar)?;
    run_blocking(move || meta::set_tags(&space, &pass, &req.path, req.tags)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MetaUpdate {
    path: String,
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct PutMetaBulkRequest {
    updates: Vec<MetaUpdate>,
}

/// Apply a batch of tag updates atomically: one decrypt + encrypt + commit
/// regardless of how many files are touched.
async fn put_meta_bulk(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<PutMetaBulkRequest>,
) -> AppResult<StatusCode> {
    let (pass, space) = require_session(&state, &jar)?;
    let updates: Vec<(String, Vec<String>)> =
        req.updates.into_iter().map(|u| (u.path, u.tags)).collect();
    run_blocking(move || meta::set_tags_bulk(&space, &pass, updates)).await?;
    Ok(StatusCode::NO_CONTENT)
}
