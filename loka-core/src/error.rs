//! Error types for loka-core.

use thiserror::Error;

/// Errors that can occur in the core triple storage engine.
#[derive(Debug, Error)]
pub enum CoreError {
    /// An IRI string was invalid.
    #[error("invalid IRI: {0}")]
    InvalidIri(String),

    /// A triple referenced an ID that does not exist in the dictionary.
    #[error("unknown ID: {0}")]
    UnknownId(u64),

    /// Attempted to insert a duplicate triple.
    #[error("duplicate triple")]
    DuplicateTriple,

    /// Storage I/O error.
    #[error("storage error: {0}")]
    Storage(#[from] std::io::Error),

    /// Sled storage error.
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),

    /// A stored byte sequence had an unexpected length (corrupt data).
    #[error("corrupt stored value: expected {expected} bytes, got {actual}")]
    CorruptValue { expected: usize, actual: usize },

    /// A batch tried to persist a term under an id that disagrees with the
    /// store's existing term dictionary (the term is stored under another id,
    /// or the id already names another term). Writing it would make the SPO
    /// keys point at the wrong terms after a reopen, so the batch is refused.
    #[error("term id conflict: {0}")]
    TermIdConflict(String),

    /// A temporal literal string could not be parsed.
    #[error("invalid temporal literal: {0}")]
    InvalidTemporal(String),

    /// A triple carried a per-query computed value (`InlineType::Computed`).
    ///
    /// Those ids index a table that only exists while their query runs, so a
    /// stored one would later resolve to an unrelated value — corruption, not a
    /// wrong answer. Nothing produces such a triple today (SPARQL update is
    /// INSERT/DELETE DATA over literal triples only); this exists so that
    /// `INSERT … WHERE`, when it is built, cannot introduce the hazard quietly.
    #[error("cannot store a computed value: id {0} is a per-query value, not a term")]
    ComputedValueNotStorable(u64),
}

pub type Result<T> = std::result::Result<T, CoreError>;
