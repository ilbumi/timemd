//! Turning core errors into HTTP responses.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use timemd_core::Error;

/// An HTTP-shaped failure. Every handler returns `Result<_, ApiError>`.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // A 5xx means the markdown tree is unreadable, which is worth a log line
        // even on a single-user box: the user's next move is to look at a file.
        if self.status.is_server_error() {
            tracing::error!(status = %self.status, "{}", self.message);
        }
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        let status = match &error {
            Error::UnknownProject(_) | Error::UnknownSession { .. } => StatusCode::NOT_FOUND,
            Error::DuplicateProject(_) => StatusCode::CONFLICT,
            Error::Io { .. } | Error::Frontmatter { .. } | Error::Parse { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        Self::new(status, error.to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
