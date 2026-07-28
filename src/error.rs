//! The crate's error type.
//!
//! Every fallible path surfaces through [`Error`]. Variants are public API: the
//! failure *modes* are normative (see `docs/specs/frame/api-contract.md` §7), so
//! adding one is a behavior change, not a refactor.

use thiserror::Error as ThisError;

/// A Modbus encoding or decoding failure.
#[derive(Debug, Clone, PartialEq, Eq, ThisError)]
pub enum Error {
    /// Input ended before the layout was satisfied (FR-R-131).
    #[error("truncated input: expected {expected} byte(s), supplied {supplied}")]
    Truncated {
        /// Bytes the layout requires.
        expected: usize,
        /// Bytes actually supplied.
        supplied: usize,
    },

    /// Input carried more bytes than the layout requires (FR-R-132).
    #[error("{extra} trailing byte(s) after a complete frame")]
    TrailingBytes {
        /// Surplus byte count.
        extra: usize,
    },

    /// A structural parse failure with no more specific cause.
    #[error("malformed frame")]
    Malformed,
}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, Error>;
