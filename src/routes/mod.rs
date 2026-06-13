pub mod agent;
pub mod auth;
pub mod files;
pub mod search;
pub mod static_files;

use axum::Router;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Run blocking work (file I/O, age decrypt, git, scrypt) on the blocking pool
/// so async workers aren't pinned. A panic or cancellation surfaces as
/// `Internal`.
pub(crate) async fn run_blocking<F, T>(f: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(format!("blocking join: {e}")))?
}

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
