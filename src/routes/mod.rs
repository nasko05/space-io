pub mod auth;
pub mod files;
pub mod static_files;

use axum::Router;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .merge(auth::router())
        .merge(files::router())
        .with_state(state);

    Router::new()
        .nest("/api", api)
        .fallback(static_files::handler)
}
