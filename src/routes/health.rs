use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

/// Liveness probe for the container `HEALTHCHECK`, the shared reverse proxy, and
/// the CD smoke test. Deliberately unauthenticated and state-free: it proves
/// only that the HTTP server is bound and routing, so a deploy that fails to
/// boot (panics, can't bind, missing bundle) is caught loudly instead of
/// silently replacing healthy containers. Kept outside `/api` so probes never
/// touch the session-auth surface.
pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub fn router() -> Router {
    Router::new().route("/healthz", get(healthz))
}
