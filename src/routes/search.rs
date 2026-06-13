use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::routes::auth::require_session;
use crate::routes::run_blocking;
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
    let (pass, space) = require_session(&state, &jar)?;
    let hits = run_blocking(move || search::search(&space, &pass, &q.q))
        .await?
        .into_iter()
        .map(|hit| SearchHitDto {
            path: hit.path,
            title: hit.title,
            snippet: hit.snippet,
        })
        .collect();
    Ok(Json(SearchResponse { hits }))
}
