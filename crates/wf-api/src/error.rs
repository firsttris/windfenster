use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use ts_rs::TS;

/// What went wrong, as a code: the SPA shows the text for it in the viewer's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    NotFound,
    InvalidCoordinates,
    /// A time span that is reversed or too long
    InvalidRange,
    Internal,
}

/// Body of every API error response.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ErrorBody {
    pub code: ErrorCode,
}

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    InvalidCoordinates,
    InvalidRange,
    /// Logged with its cause; the client only learns that something went wrong.
    Internal(String),
}

impl ApiError {
    pub fn internal(err: impl std::fmt::Display) -> Self {
        Self::Internal(err.to_string())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self::internal(err)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(format!("{err:#}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, ErrorCode::NotFound),
            ApiError::InvalidCoordinates => {
                (StatusCode::BAD_REQUEST, ErrorCode::InvalidCoordinates)
            }
            ApiError::InvalidRange => (StatusCode::BAD_REQUEST, ErrorCode::InvalidRange),
            ApiError::Internal(cause) => {
                tracing::error!("request failed: {cause}");
                (StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::Internal)
            }
        };
        (status, Json(ErrorBody { code })).into_response()
    }
}
