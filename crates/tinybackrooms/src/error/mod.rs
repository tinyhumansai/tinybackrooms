//! Crate-wide error and result types.
//!
//! Every fallible public function in this crate returns [`Result`], and every
//! failure mode is a distinct [`Error`] variant. Callers match on variants;
//! message text is not a stable API.
//!
//! The variants fall into three groups: a request the engine refuses before
//! spending any model calls, a backend (model or memory) that failed, and a run
//! that completed without producing anything usable.

/// Errors returned by this crate.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The creative brief was empty or only whitespace.
    #[error("brief must not be empty")]
    EmptyBrief,
    /// `variations` was zero, so there is nothing to run.
    #[error("variations must be at least 1")]
    NoVariations,
    /// `variations` exceeded the engine's concurrency ceiling.
    #[error("variations must be at most {max}")]
    TooManyVariations {
        /// The largest accepted number of variations.
        max: u32,
    },
    /// `generations` was zero, so no conversation turn would happen.
    #[error("generations must be at least 1")]
    NoGenerations,
    /// `items_per_variation` was zero.
    #[error("items_per_variation must be at least 1")]
    NoItems,
    /// A scorer weight was negative, not finite, or every weight was zero.
    #[error("scorer weights must be finite, non-negative, and not all zero")]
    InvalidWeights,
    /// `select_top_k` was zero, which would discard every item.
    #[error("select_top_k must be at least 1")]
    InvalidTopK,
    /// The output schema is not a valid JSON Schema.
    #[error("output schema is invalid: {0}")]
    InvalidSchema(String),
    /// A model backend failed to answer a turn.
    #[error("conversant failed: {0}")]
    Conversant(String),
    /// The memory backend failed to recall or store.
    #[error("memory failed: {0}")]
    Memory(String),
    /// Every variation ended without a single item that satisfies the schema.
    #[error("no variation produced an item that satisfies the output schema")]
    NoValidItems,
}

/// The crate's standard result type.
///
/// Use this alias in public signatures instead of spelling out
/// `std::result::Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test;
