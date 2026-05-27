use axum::extract::{Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::routes::auth::require_passphrase;
use crate::space::{create, excerpt, read, tree, write};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/files/tree", get(get_tree))
        .route("/files/read", get(get_read))
        .route("/files/write", put(put_write))
        .route("/files/create", post(post_create))
        .route("/files/excerpts", get(get_excerpts))
}

#[derive(Serialize)]
struct TreeResponse {
    tree: Vec<tree::TreeNode>,
}

async fn get_tree(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Json<TreeResponse>> {
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
