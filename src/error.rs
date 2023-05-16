use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::fmt;

/// Custom error class.
#[derive(Debug, Clone)]
pub struct HttpError {
    pub status_code: StatusCode,
    pub message: String,
}

impl HttpError {
    pub fn bad_request(message: &str) -> HttpError {
        HttpError {
            status_code: StatusCode::BAD_REQUEST,
            message: message.to_string(),
        }
    }

    pub fn not_found(message: &str) -> HttpError {
        HttpError {
            status_code: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }

    pub fn internal_server_error(message: &str) -> HttpError {
        HttpError {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.to_string(),
        }
    }
}

impl Serialize for HttpError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("HttpError", 2)?;
        state.serialize_field("status_code", &self.status_code.as_u16())?;
        state.serialize_field("message", &self.message)?;
        state.end()
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Code {}: {}", self.status_code.as_u16(), self.message)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.status_code, Json(self)).into_response()
    }
}
