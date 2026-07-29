//! The ADU framing abstraction (FR-R-120, FR-R-121).

use alloc::vec::Vec;
use core::fmt::Debug;

use crate::error::Result;
use crate::frame::pdu::{RequestPdu, ResponsePdu};

/// How the end of an ADU is determined (FR-R-122).
///
/// This is a description, not a reader: it names the rule so the transport can
/// apply it, and performs no I/O itself. That keeps the rule testable on byte
/// vectors and available where there is no I/O at all.
///
/// No `PartialEq`: one variant carries a function pointer, whose address says
/// nothing meaningful about equality. Match on the variant instead.
#[derive(Debug, Clone, Copy)]
pub enum AduBoundary {
    /// A fixed-size prefix determines the length. `prefix` bytes are read, and
    /// `total` applied to them yields the length of the whole ADU, that prefix
    /// included.
    Prefixed {
        /// Bytes needed before the length can be computed.
        prefix: usize,
        /// Maps those bytes to the whole ADU's length, rejecting a length the
        /// framing does not permit.
        total: fn(&[u8]) -> Result<usize>,
    },
    /// The ADU runs from `start` to the first occurrence of `end` after it.
    Delimited {
        /// Byte that opens an ADU; anything before it is not part of one.
        start: u8,
        /// Byte sequence that closes an ADU.
        end: &'static [u8],
    },
    /// The ADU ends when the line falls silent for long enough. How long is a
    /// property of the port, not of the framing (TR-R-011).
    Silence,
}

/// One of the three ways a PDU is wrapped for transmission.
///
/// The framings differ in what they put around the PDU and in what identifies
/// the peer, so the header is an associated type rather than a fixed field:
/// RTU and ASCII carry a 1-byte address (FR-R-096, FR-R-117), TCP carries a
/// transaction identifier and a unit identifier (FR-R-101).
///
/// Decoding yields the header and the PDU separately (FR-R-121), so a caller
/// can route on the header without re-encoding anything. Request and response
/// have their own methods because a PDU is not self-describing — the caller
/// states the direction (FR-R-005).
pub trait Framing {
    /// What identifies the peer in this framing.
    type Header: Clone + PartialEq + Debug;

    /// Largest ADU this framing permits (FR-R-091, FR-R-104, FR-R-113).
    const MAX_ADU_LEN: usize;

    /// Decode an ADU carrying a request.
    ///
    /// # Errors
    ///
    /// Fails if the framing is malformed, the checksum does not match, or the
    /// PDU within it does not decode.
    fn decode_request(bytes: &[u8]) -> Result<(Self::Header, RequestPdu)>;

    /// Encode a request into an ADU.
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode.
    fn encode_request(header: &Self::Header, pdu: &RequestPdu) -> Result<Vec<u8>>;

    /// Decode an ADU carrying a response.
    ///
    /// # Errors
    ///
    /// Fails if the framing is malformed, the checksum does not match, or the
    /// PDU within it does not decode.
    fn decode_response(bytes: &[u8]) -> Result<(Self::Header, ResponsePdu)>;

    /// Encode a response into an ADU.
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode.
    fn encode_response(header: &Self::Header, pdu: &ResponsePdu) -> Result<Vec<u8>>;

    /// How the end of one of this framing's ADUs is determined (FR-R-122).
    fn boundary() -> AduBoundary;
}
