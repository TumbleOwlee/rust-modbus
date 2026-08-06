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

    /// An I/O failure on a socket or a serial port (TR-R-040).
    ///
    /// The kind is carried rather than the [`std::io::Error`] itself: this enum
    /// is compared for equality throughout the crate's tests, and `io::Error`
    /// implements no `PartialEq`. The kind is the part a caller matches on.
    #[cfg(feature = "std")]
    #[error("I/O error: {kind}")]
    Io {
        /// What the operating system reported.
        kind: std::io::ErrorKind,
    },

    /// An operation did not complete within its time limit (TR-R-021,
    /// TR-R-041).
    #[cfg(feature = "std")]
    #[error("{what} timed out")]
    Timeout {
        /// The operation that timed out.
        what: &'static str,
    },

    /// The peer closed the connection part-way through an ADU (TR-R-014).
    ///
    /// A close *between* two ADUs is an ordinary end of stream, not this.
    #[cfg(feature = "std")]
    #[error("connection closed mid-frame")]
    ConnectionClosed,

    /// A configuration field held a value the transport cannot use
    /// (TR-R-031).
    #[cfg(feature = "std")]
    #[error("invalid configuration for {field}")]
    Configuration {
        /// The offending field.
        field: &'static str,
    },

    /// The server refused the request with a protocol exception (CL-R-040,
    /// CL-R-041).
    #[cfg(feature = "std")]
    #[error("server returned exception {exception:?} for function {function:?}")]
    Exception {
        /// The function the exception answers.
        function: crate::frame::FunctionCode,
        /// The exception the server chose.
        exception: crate::frame::ExceptionCode,
    },

    /// A response matched the request's header but carried another function's
    /// body (CL-R-022).
    #[cfg(feature = "std")]
    #[error("expected a response to {expected:?}, got {actual:?}")]
    UnexpectedFunction {
        /// The function requested.
        expected: crate::frame::FunctionCode,
        /// The function the response carried.
        actual: crate::frame::FunctionCode,
    },

    /// A function code or MEI type whose length cannot be derived from its bytes
    /// (FR-R-148). This includes FC 8 in both directions, FC 43 with any MEI type
    /// other than 14, and every custom function code. A device using any of them
    /// behind a transparent gateway is not reachable through RTU-over-stream framing.
    #[error("cannot derive ADU length from function code {function:#04x}")]
    IndeterminateLength {
        /// The function code whose length cannot be determined.
        function: u8,
    },

    /// What the peer will send next is no longer known, so no further request
    /// may be issued on this connection (CL-R-031, CL-R-032).
    #[cfg(feature = "std")]
    #[error("the exchange is desynchronized; a new connection is required")]
    Desynchronized,

    /// A blocking method was called from a thread that already drives an async
    /// runtime (CL-R-075). Blocking on a runtime from inside one deadlocks or
    /// panics, so the blocking client refuses before touching the transport. Use
    /// the async `Client` here instead.
    #[cfg(feature = "sync")]
    #[error("a blocking call was made from inside an async runtime")]
    BlockingInAsyncContext,

    /// The kernel's RS-485 direction-control mode could not be applied: the
    /// target is not Linux, or the driver's `TIOCSRS485` ioctl reported the
    /// mode is not implemented (TR-R-054).
    #[cfg(feature = "rs485")]
    #[error("RS-485 kernel mode is not supported on this platform or by this driver")]
    Rs485Unsupported,

    /// A TLS handshake failed, distinct from a TCP connect failure (`Io`) or
    /// an expired timeout (`Timeout`) (TR-R-062, TR-R-067).
    #[cfg(feature = "tls")]
    #[error("TLS handshake failed")]
    TlsHandshake,
}

#[cfg(feature = "std")]
impl Error {
    /// Whether this failure ends the byte stream itself, rather than costing
    /// one frame.
    ///
    /// An I/O failure, a closed connection or a timeout say nothing about
    /// framing: the stream is over, or what the peer sends next is unaccounted
    /// for, whichever framing is in use. Every other variant is a *frame*
    /// failure, and whether it is survivable is then the framing's answer
    /// (FR-R-144) — which is why the client and the server ask this first and
    /// the boundary rule second (CL-R-023, SV-R-050).
    pub(crate) fn ends_stream(&self) -> bool {
        matches!(
            *self,
            Self::Io { .. } | Self::ConnectionClosed | Self::Timeout { .. }
        )
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io { kind: error.kind() }
    }
}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    /// A stream failure is distinguished from a frame failure by variant, not
    /// by framing: the two areas that branch on it must agree, so the
    /// predicate lives in one place.
    fn ut_stream_failures_are_distinguished_from_frame_failures() {
        assert!(
            Error::Io {
                kind: std::io::ErrorKind::BrokenPipe
            }
            .ends_stream()
        );
        assert!(Error::ConnectionClosed.ends_stream());
        assert!(Error::Timeout { what: "response" }.ends_stream());

        // Every one of these is one frame's worth of damage.
        assert!(!Error::Malformed.ends_stream());
        assert!(
            !Error::Checksum {
                expected: 1,
                actual: 2
            }
            .ends_stream()
        );
        assert!(
            !Error::Truncated {
                expected: 8,
                supplied: 3
            }
            .ends_stream()
        );
        assert!(!Error::InvalidFunctionCode(0x99).ends_stream());
    }

    #[cfg(feature = "tls")]
    #[test]
    /// TR-R-067 — TLS handshake failure is a distinct variant, separate from
    /// `Io` and `Timeout`.
    fn ut_tls_handshake_is_distinct_from_io_and_timeout() {
        assert_ne!(
            Error::TlsHandshake,
            Error::Io {
                kind: std::io::ErrorKind::Other
            }
        );
        assert_ne!(Error::TlsHandshake, Error::Timeout { what: "connect" });
    }

    #[test]
    /// TR-R-040 — an I/O failure surfaces as a typed variant carrying the
    /// kind the platform reported, not as a formatted string.
    fn ut_io_error_maps_to_kind() {
        let error: Error = std::io::Error::from(std::io::ErrorKind::ConnectionRefused).into();
        assert_eq!(
            error,
            Error::Io {
                kind: std::io::ErrorKind::ConnectionRefused,
            }
        );
    }
}
