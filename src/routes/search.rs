use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::routes::auth::require_passphrase;
use crate::space::search;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/search", get(get_search))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

#[derive(Serialize)]
struct SearchHitDto {
    path: String,
    title: Option<String>,
    snippet: String,
}

#[derive(Serialize)]
struct SearchResponse {
    hits: Vec<SearchHitDto>,
}

async fn get_search(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let pass = require_passphrase(&state, &jar)?;
    let space = state.space.clone();
    let hits = tokio::task::spawn_blocking(move || search::search(&space, &pass, &q.q))
        .await
        .map_err(|e| AppError::Internal(format!("search join: {e}")))??
        .into_iter()
        .map(|h| SearchHitDto {
            path: h.path,
            title: h.title,
            snippet: h.snippet,
        })
        .collect();
    Ok(Json(SearchResponse { hits }))
}
