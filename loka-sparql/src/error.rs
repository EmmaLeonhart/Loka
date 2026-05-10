//! Error types for loka-sparql.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SparqlError {
    #[error("parse error at position {position}: {message}")]
    Parse { position: usize, message: String },

    #[error("unknown prefix: {0}")]
    UnknownPrefix(String),

    #[error("query execution error: {0}")]
    Execution(String),

    #[error("vector error: {0}")]
    Vector(String),

    #[error("HNSW error: {0}")]
    Hnsw(#[from] loka_hnsw::HnswError),

    #[error("core error: {0}")]
    Core(#[from] loka_core::CoreError),

    #[error("query timeout exceeded")]
    Timeout,
}

pub type Result<T> = std::result::Result<T, SparqlError>;
