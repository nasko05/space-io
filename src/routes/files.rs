use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::routes::auth::require_passphrase;
use crate::space::{read, tree};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/files/tree", get(get_tree))
        .route("/files/read", get(get_read))
}

#[derive(Serialize)]
struct TreeResponse {
    tree: Vec<tree::TreeNode>,
}

async fn get_tree(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Json<TreeResponse>> {
    // Auth gate (we don't need the passphrase here, but unlock is required).
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
