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
use crate::routes::auth::require_passphrase;
use crate::space::{
    create, delete as delete_mod, download, excerpt, history, meta, mkdir, read, rename, tree,
    upload, write,
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
        .route("/files/move", post(post_move))
        .route("/files/delete", delete(delete_file))
        .route("/files/mkdir", post(post_mkdir))
        .route("/files/meta", get(get_meta).put(put_meta))
}

#[derive(Serialize)]
struct TreeResponse {
    tree: Vec<tree::TreeNode>,
}

async fn get_tree(State(state): State<AppState>, jar: CookieJar) -> AppResult<Json<TreeResponse>> {
    require_passphrase(&state, &jar)?;
    let tree = tree::build_tree(&state.space)?;
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
    let pass = require_passphrase(&state, &jar)?;
    let result = read::read_file(&state.space, &pass, &q.path)?;
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
    let pass = require_passphrase(&state, &jar)?;
    let result = write::write_file(
        &state.space,
        &pass,
        &req.path,
        &req.content,
        req.message.as_deref(),
    )?;
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
    let pass = require_passphrase(&state, &jar)?;
    let result = create::create_file(&state.space, &pass, &req.folder, req.title.as_deref())?;
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
    let pass = require_passphrase(&state, &jar)?;
    let raw = excerpt::build_excerpts(&state.space, &pass)?;
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
    let pass = require_passphrase(&state, &jar)?;

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

    let mut results = Vec::with_capacity(files.len());
    for (name, bytes) in files {
        let r = upload::store_upload(&state.space, &pass, &folder, &name, &bytes)?;
        results.push(UploadResult {
            path: r.path,
            size: r.size,
        });
    }
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
    let pass = require_passphrase(&state, &jar)?;
    let file = download::fetch_decrypted(&state.space, &pass, &q.path)?;
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
    require_passphrase(&state, &jar)?;
    let entries = history::file_history(&state.space, &q.path)?
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
    let pass = require_passphrase(&state, &jar)?;
    let r = rename::rename_path(&state.space, &pass, &req.from, &req.to)?;
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
    let pass = require_passphrase(&state, &jar)?;
    let r = delete_mod::delete_to_trash(&state.space, &pass, &req.path)?;
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
    require_passphrase(&state, &jar)?;
    mkdir::create_folder(&state.space, &req.path)?;
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
    let pass = require_passphrase(&state, &jar)?;
    let idx = meta::load(&state.space, &pass)?;
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
    let pass = require_passphrase(&state, &jar)?;
    meta::set_tags(&state.space, &pass, &req.path, req.tags)?;
    Ok(StatusCode::NO_CONTENT)
}
