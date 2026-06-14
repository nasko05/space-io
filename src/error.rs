use axum::http::{header, HeaderValue, StatusCode};
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
    #[error("too many requests; retry in {retry_after_secs}s")]
    TooManyRequests { retry_after_secs: u64 },
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
            AppError::TooManyRequests { .. } => {
                (StatusCode::TOO_MANY_REQUESTS, "too_many_requests")
            }
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "io"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for AppError {
    /// Server errors log their full chain server-side but collapse to a generic
    /// client message, so we never leak filesystem paths, libgit2, or age
    /// internals over the wire.
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        }
        let retry_after = if let AppError::TooManyRequests { retry_after_secs } = &self {
            Some(*retry_after_secs)
        } else {
            None
        };
        let message = match &self {
            AppError::Io(_) | AppError::Internal(_) => "internal server error".to_string(),
            _ => self.to_string(),
        };
        let body = Json(json!({
            "error": { "code": code, "message": message }
        }));
        let mut response = (status, body).into_response();
        if let Some(secs) = retry_after {
            if let Ok(value) = HeaderValue::from_str(&secs.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    fn err_code(err: AppError) -> (StatusCode, String) {
        let (status, code, _) = err_parts(err);
        (status, code)
    }

    fn err_parts(err: AppError) -> (StatusCode, String, String) {
        let res = err.into_response();
        let status = res.status();
        let body = res.into_body();
        let bytes = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { to_bytes(body, usize::MAX).await.unwrap() });
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (
            status,
            json["error"]["code"].as_str().unwrap().to_string(),
            json["error"]["message"].as_str().unwrap().to_string(),
        )
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
    fn too_many_requests_is_429_with_retry_after() {
        let res = AppError::TooManyRequests {
            retry_after_secs: 42,
        }
        .into_response();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            res.headers()
                .get(header::RETRY_AFTER)
                .map(|v| v.to_str().unwrap()),
            Some("42")
        );
    }

    #[test]
    fn bad_request_maps_to_400() {
        let (s, c) = err_code(AppError::BadRequest("oops".into()));
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(c, "bad_request");
    }

    #[test]
    fn io_maps_to_500_without_leaking_inner_message() {
        let (s, c, m) = err_parts(AppError::Io(std::io::Error::other("disk on fire")));
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(c, "io");
        assert!(
            !m.contains("disk on fire"),
            "internal IO detail leaked to client: {m}",
        );
        assert_eq!(m, "internal server error");
    }

    #[test]
    fn internal_collapses_to_generic_message_over_the_wire() {
        let err = AppError::Internal("kaboom".into());
        assert_eq!(err.to_string(), "internal: kaboom");
        let (s, c, m) = err_parts(err);
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(c, "internal");
        assert!(
            !m.contains("kaboom"),
            "internal detail leaked to client: {m}",
        );
    }

    #[test]
    fn bad_request_message_is_passed_through() {
        let (_, _, m) = err_parts(AppError::BadRequest("name must not be empty".into()));
        assert_eq!(m, "bad request: name must not be empty");
    }
}
