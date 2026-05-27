use axum::body::Body;
use axum::extract::{Multipart, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use axum::routing::delete;

use crate::error::{AppError, AppResult};
use crate::routes::auth::require_session;
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
        .route("/files/create", post(post_create))
        .route("/files/excerpts", get(get_excerpts))
        .route("/files/upload", post(post_upload))
        .route("/files/download", get(get_download))
        .route("/files/history", get(get_history))
        .route("/files/rollback", post(post_rollback))
        .route("/files/move", post(post_move))
        .route("/files/delete", delete(delete_file))
        .route("/files/mkdir", post(post_mkdir))
        .route("/files/meta", get(get_meta).put(put_meta))
}

/// Run blocking work (file I/O, age decrypt, git) on the dedicated
/// blocking pool so we don't pin async workers. Joining the task is what
/// surfaces panics or cancellation as `AppError::Internal`.
async fn blocking<F, T>(f: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(format!("blocking join: {e}")))?
}

#[derive(Serialize)]
struct TreeResponse {
    tree: Vec<tree::TreeNode>,
}

async fn get_tree(State(state): State<AppState>, jar: CookieJar) -> AppResult<Json<TreeResponse>> {
    let (_, space) = require_session(&state, &jar)?;
    let tree = blocking(move || tree::build_tree(&space)).await?;
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
    let result = blocking(move || read::read_file(&space, &pass, &q.path)).await?;
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
    message: Option<String>,
}

#[derive(Serialize)]
struct WriteResponse {
    path: String,
    updated: String,
}

async fn put_write(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<WriteRequest>,
) -> AppResult<Json<WriteResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let result = blocking(move || {
        write::write_file(
            &space,
            &pass,
            &req.path,
            &req.content,
            req.message.as_deref(),
        )
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
        blocking(move || create::create_file(&space, &pass, &req.folder, req.title.as_deref()))
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
    let raw = blocking(move || excerpt::build_excerpts(&space, &pass)).await?;
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

async fn post_upload(
    State(state): State<AppState>,
    jar: CookieJar,
    mut multipart: Multipart,
) -> AppResult<Json<UploadResponse>> {
    let (pass, space) = require_session(&state, &jar)?;

    let mut folder: Option<String> = None;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

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
                    "{} exceeds {MAX_UPLOAD_BYTES} bytes",
                    filename
                )));
            }
            files.push((filename, bytes.to_vec()));
        }
    }
    if files.is_empty() {
        return Err(AppError::BadRequest("no files in multipart body".into()));
    }
    let folder = folder.unwrap_or_else(|| DEFAULT_UPLOAD_FOLDER.to_string());

    let results = blocking(move || {
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
    let file = blocking(move || download::fetch_decrypted(&space, &pass, &q.path)).await?;
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
    let entries = blocking(move || history::file_history(&space, &q.path))
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
    let r = blocking(move || rollback::rollback_to(&space, &pass, &req.path, &req.commit)).await?;
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

async fn post_move(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<MoveRequest>,
) -> AppResult<Json<MoveResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let r = blocking(move || rename::rename_path(&space, &pass, &req.from, &req.to)).await?;
    Ok(Json(MoveResponse {
        path: r.path,
        is_directory: r.is_directory,
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

async fn delete_file(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<DeleteRequest>,
) -> AppResult<Json<DeleteResponse>> {
    let (pass, space) = require_session(&state, &jar)?;
    let r = blocking(move || delete_mod::delete_to_trash(&space, &pass, &req.path)).await?;
    Ok(Json(DeleteResponse {
        trash_path: r.trash_path,
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
    blocking(move || mkdir::create_folder(&space, &req.path)).await?;
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
    let idx = blocking(move || meta::load(&space, &pass)).await?;
    let meta = idx
        .paths
        .into_iter()
        .map(|(k, v)| (k, MetaItem { tags: v.tags }))
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
    blocking(move || meta::set_tags(&space, &pass, &req.path, req.tags)).await?;
    Ok(StatusCode::NO_CONTENT)
}
