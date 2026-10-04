//! Mapping engine errors to HTTP responses.
//!
//! Every error body has one shape, `{"error": {"code": ..., "message": ...}}`,
//! so a client branches on `code` and never parses prose.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tinybackrooms::Error;

/// An error ready to become a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl ApiError {
    /// The request body was not valid JSON for a `GenerateRequest`.
    pub(crate) fn invalid_json(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_json",
            message: message.into(),
        }
    }

    /// The JSON body of the response.
    pub(crate) fn body(&self) -> Value {
        json!({"error": {"code": self.code, "message": self.message}})
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        let (status, code) = match &error {
            Error::EmptyBrief
            | Error::NoVariations
            | Error::TooManyVariations { .. }
            | Error::NoGenerations
            | Error::NoItems
            | Error::InvalidWeights
            | Error::InvalidTopK
            | Error::InvalidSchema(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Error::NoValidItems => (StatusCode::UNPROCESSABLE_ENTITY, "no_valid_items"),
            Error::Conversant(_) | Error::Memory(_) => (StatusCode::BAD_GATEWAY, "backend_error"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        Self {
            status,
            code,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body())).into_response()
    }
}
