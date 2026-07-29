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

    /// A function code that cannot denote a request: 0, or 128–255
    /// (FR-R-014, FR-R-015).
    #[error("invalid function code {0}")]
    InvalidFunctionCode(u8),

    /// A custom or general value was given a code the crate already names, which
    /// would give one wire byte two representations (FR-R-013, FR-R-084).
    #[error("code {0} is named and must not be carried as a custom value")]
    ReservedCode(u8),

    /// A field or PDU whose length is fixed by its layout did not have it
    /// (FR-R-085, FR-R-105, FR-R-106).
    #[error("invalid length: expected {expected}, got {actual}")]
    InvalidLength {
        /// Bytes the layout fixes.
        expected: usize,
        /// Bytes actually present.
        actual: usize,
    },

    /// A field fell outside the range its function code fixes (FR-R-021,
    /// FR-R-022, FR-R-031, FR-R-033, FR-R-038, FR-R-042, FR-R-056, FR-R-074).
    #[error("{field} is {value}, outside the permitted range {min}..={max}")]
    OutOfRange {
        /// The field that was out of range.
        field: &'static str,
        /// The offending value.
        value: u32,
        /// Smallest permitted value.
        min: u32,
        /// Largest permitted value.
        max: u32,
    },

    /// An ADU's checksum did not match the bytes it covers (FR-R-095,
    /// FR-R-115).
    #[error("checksum mismatch: expected {expected:#06x}, computed {actual:#06x}")]
    Checksum {
        /// The checksum computed over the ADU's own bytes.
        expected: u16,
        /// The checksum the ADU carried.
        actual: u16,
    },

    /// An ASCII ADU was not framed as FR-R-116 requires.
    #[error("ASCII ADU has a malformed {element}")]
    Framing {
        /// Which part of the framing was wrong.
        element: &'static str,
    },

    /// A character outside `0`-`9`, `A`-`F`, `a`-`f` appeared where an ASCII
    /// ADU requires a hexadecimal digit (FR-R-112).
    #[error("byte {0:#04x} is not a hexadecimal character")]
    InvalidCharacter(u8),

    /// An MBAP header carried a protocol identifier other than 0 (FR-R-102).
    #[error("MBAP protocol identifier {0} is not 0")]
    ProtocolIdentifier(u16),

    /// An ADU exceeded the maximum its framing permits (FR-R-091, FR-R-104,
    /// FR-R-113).
    #[error("ADU of {len} bytes exceeds the maximum of {max}")]
    AduTooLarge {
        /// The oversized length.
        len: usize,
        /// The framing's maximum.
        max: usize,
    },

    /// A file record named a reference type other than 6 (FR-R-055).
    #[error("file record reference type {0} is not 6")]
    ReferenceType(u8),

    /// A field carried a value its layout does not define (FR-R-027, FR-R-046,
    /// FR-R-057, FR-R-061, FR-R-076).
    #[error("{field} carries the undefined value {value:#06x}")]
    IllegalValue {
        /// The field that was illegal.
        field: &'static str,
        /// The offending value.
        value: u16,
    },

    /// A byte-count field disagreed with the data present or with the value its
    /// quantity field implies (FR-R-043, FR-R-051, FR-R-054, FR-R-077).
    #[error("byte count mismatch: expected {expected}, got {actual}")]
    ByteCountMismatch {
        /// The byte count the layout implies.
        expected: usize,
        /// The byte count actually found.
        actual: usize,
    },

    /// An encoded PDU exceeded the 253-byte maximum (FR-R-002, FR-R-006).
    #[error("PDU is {len} bytes, exceeding the {max}-byte maximum")]
    PduTooLarge {
        /// The size that would have been emitted.
        len: usize,
        /// The maximum permitted size.
        max: usize,
    },

    /// A structural parse failure with no more specific cause.
    #[error("malformed frame")]
    Malformed,
}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, Error>;
