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
    /// The request needs a signed-in user
    SignedOut,
    /// This server has no public address passkeys could belong to
    PasskeysOff,
    /// The passkey's answer did not check out, or the ceremony ran out
    PasskeyRejected,
    /// A name that is empty or longer than 60 characters
    InvalidName,
    /// Wind directions outside 0–359
    InvalidFavorite,
    TooManyFavorites,
    /// An address that does not look like one
    InvalidEmail,
    /// Another account signs in with this address
    EmailTaken,
    /// Shorter than 10 or longer than 200 characters
    WeakPassword,
    /// Address or password wrong (which one is not told)
    WrongPassword,
    /// Too many failed attempts for the address; wait 15 minutes
    TooManyAttempts,
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
    SignedOut,
    PasskeysOff,
    PasskeyRejected,
    InvalidName,
    InvalidFavorite,
    TooManyFavorites,
    InvalidEmail,
    EmailTaken,
    WeakPassword,
    WrongPassword,
    TooManyAttempts,
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
            ApiError::SignedOut => (StatusCode::UNAUTHORIZED, ErrorCode::SignedOut),
            ApiError::PasskeysOff => (StatusCode::SERVICE_UNAVAILABLE, ErrorCode::PasskeysOff),
            ApiError::PasskeyRejected => (StatusCode::BAD_REQUEST, ErrorCode::PasskeyRejected),
            ApiError::InvalidName => (StatusCode::BAD_REQUEST, ErrorCode::InvalidName),
            ApiError::InvalidFavorite => (StatusCode::BAD_REQUEST, ErrorCode::InvalidFavorite),
            ApiError::TooManyFavorites => (StatusCode::CONFLICT, ErrorCode::TooManyFavorites),
            ApiError::InvalidEmail => (StatusCode::BAD_REQUEST, ErrorCode::InvalidEmail),
            ApiError::EmailTaken => (StatusCode::CONFLICT, ErrorCode::EmailTaken),
            ApiError::WeakPassword => (StatusCode::BAD_REQUEST, ErrorCode::WeakPassword),
            ApiError::WrongPassword => (StatusCode::UNAUTHORIZED, ErrorCode::WrongPassword),
            ApiError::TooManyAttempts => {
                (StatusCode::TOO_MANY_REQUESTS, ErrorCode::TooManyAttempts)
            }
            ApiError::Internal(cause) => {
                tracing::error!("request failed: {cause}");
                (StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::Internal)
            }
        };
        (status, Json(ErrorBody { code })).into_response()
    }
}
