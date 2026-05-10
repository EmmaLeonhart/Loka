//! Error types for loka-proto.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("SPARQL error: {0}")]
    Sparql(#[from] loka_sparql::SparqlError),

    #[error("core error: {0}")]
    Core(#[from] loka_core::CoreError),

    #[error("bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for ProtoError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ProtoError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ProtoError::Sparql(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ProtoError::Core(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, message).into_response()
    }
}

pub type Result<T> = std::result::Result<T, ProtoError>;
