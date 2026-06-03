pub mod agent;
pub mod auth;
pub mod files;
pub mod search;
pub mod static_files;

use axum::Router;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .merge(agent::router())
        .merge(auth::router())
        .merge(files::router())
        .merge(search::router())
        .with_state(state);

    Router::new()
        .nest("/api", api)
        .fallback(static_files::handler)
}
