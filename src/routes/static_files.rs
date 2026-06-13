use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/web/dist"]
struct Assets;

pub async fn handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(asset) = Assets::get(path) {
        return serve(path, asset);
    }
    // SPA fallback: unknown routes return index.html so the client handles deep
    // links.
    if let Some(index) = Assets::get("index.html") {
        return serve("index.html", index);
    }
    (
        StatusCode::NOT_FOUND,
        "frontend bundle is missing — run `npm run build` in web/",
    )
        .into_response()
}

fn serve(path: &str, asset: rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(asset.data.into_owned()))
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "static serve failed").into_response()
        })
}
