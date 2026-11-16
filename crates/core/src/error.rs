//! Errors, and how each one is reported over HTTP: `{"detail": ...}` with a status code,
//! the same shape the UI already handles.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::dice::DiceError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 404: a record, entry or file that doesn't exist.
    #[error("{0}")]
    NotFound(String),
    /// 400: input that's well-formed but not acceptable (a bad dice expression, a link to
    /// a missing record, an invalid import).
    #[error("{0}")]
    Invalid(String),
    /// 422: a request body or query parameter that doesn't fit the expected shape.
    /// `location` is where, e.g. `["body", "count"]`.
    #[error("{message}")]
    Unprocessable { location: Vec<String>, message: String },
    /// 403: a write from another website open in the same browser.
    #[error("cross-origin request refused")]
    CrossOrigin,
    /// 413: a request body over the limit.
    #[error("request too large")]
    TooLarge,
    /// 500: something that shouldn't happen, such as a database failure.
    #[error("{0}")]
    Internal(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub fn not_found(message: impl Into<String>) -> Error {
    Error::NotFound(message.into())
}

pub fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

impl From<DiceError> for Error {
    fn from(e: DiceError) -> Self {
        Error::Invalid(e.0)
    }
}

impl From<sea_orm::DbErr> for Error {
    fn from(e: sea_orm::DbErr) -> Self {
        Error::Internal(format!("database error: {e}"))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Internal(format!("file error: {e}"))
    }
}

impl Error {
    pub fn status(&self) -> StatusCode {
        match self {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Invalid(_) => StatusCode::BAD_REQUEST,
            Error::Unprocessable { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Error::CrossOrigin => StatusCode::FORBIDDEN,
            Error::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Error::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The `detail` value: a message, or for 422 a list like FastAPI's validation errors.
    pub fn detail(&self) -> Value {
        match self {
            Error::Unprocessable { location, message } => {
                json!([{"loc": location, "msg": message, "type": "value_error"}])
            }
            other => json!(other.to_string()),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        if let Error::Internal(message) = &self {
            eprintln!("questodian: {message}");
        }
        (self.status(), Json(json!({"detail": self.detail()}))).into_response()
    }
}
