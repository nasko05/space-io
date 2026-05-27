use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("unauthorized")]
    Unauthorized,
    #[error("wrong passphrase")]
    WrongPassphrase,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("internal: {0}")]
    Internal(String),
}

impl AppError {
    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::WrongPassphrase => (StatusCode::UNAUTHORIZED, "wrong_passphrase"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "io"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        }
        let body = Json(json!({
            "error": { "code": code, "message": self.to_string() }
        }));
        (status, body).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    fn err_code(err: AppError) -> (StatusCode, String) {
        let res = err.into_response();
        let status = res.status();
        let body = res.into_body();
        let bytes = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { to_bytes(body, usize::MAX).await.unwrap() });
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (status, json["error"]["code"].as_str().unwrap().to_string())
    }

    #[test]
    fn not_found_maps_to_404() {
        let (s, c) = err_code(AppError::NotFound);
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(c, "not_found");
    }

    #[test]
    fn forbidden_maps_to_403() {
        let (s, c) = err_code(AppError::Forbidden);
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(c, "forbidden");
    }

    #[test]
    fn unauthorized_maps_to_401() {
        let (s, c) = err_code(AppError::Unauthorized);
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert_eq!(c, "unauthorized");
    }

    #[test]
    fn wrong_passphrase_is_401_with_distinct_code() {
        let (s, c) = err_code(AppError::WrongPassphrase);
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert_eq!(c, "wrong_passphrase");
    }

    #[test]
    fn bad_request_maps_to_400() {
        let (s, c) = err_code(AppError::BadRequest("oops".into()));
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(c, "bad_request");
    }

    #[test]
    fn io_maps_to_500() {
        let (s, c) = err_code(AppError::Io(std::io::Error::other("disk on fire")));
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(c, "io");
    }

    #[test]
    fn internal_maps_to_500_with_message() {
        let err = AppError::Internal("kaboom".into());
        assert_eq!(err.to_string(), "internal: kaboom");
        let (s, c) = err_code(err);
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(c, "internal");
    }
}
