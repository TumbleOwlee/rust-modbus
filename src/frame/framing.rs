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

impl AduBoundary {
    /// Whether the next frame boundary is findable from the wire alone
    /// (FR-R-144).
    ///
    /// Silence and delimiters are properties of the line: after any failure the
    /// next boundary is found without reference to the frame that failed, so one
    /// bad frame costs exactly one frame. A length field is carried *by* that
    /// frame, so losing it loses every boundary after it too.
    ///
    /// The client and the server consult this to decide whether an undecodable
    /// frame costs one frame or the whole link (CL-R-023, SV-R-050).
    #[must_use]
    pub fn is_self_locating(&self) -> bool {
        match *self {
            Self::Delimited { .. } | Self::Silence => true,
            Self::Prefixed { .. } => false,
        }
    }
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

    /// Encode a request into an ADU, appending to `out` (FR-R-140).
    ///
    /// This is the primitive: [`Self::encode_request`] is defined in terms of
    /// it, so the two cannot describe different bytes. The framing's maximum
    /// ADU length is reserved before the first byte is written (FR-R-141), and
    /// a failure truncates `out` back to the length it had on entry
    /// (FR-R-142).
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode.
    fn encode_request_into(
        header: &Self::Header,
        pdu: &RequestPdu,
        out: &mut Vec<u8>,
    ) -> Result<()>;

    /// Encode a request into an ADU, allocating a buffer for it (FR-R-140).
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode.
    fn encode_request(header: &Self::Header, pdu: &RequestPdu) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        Self::encode_request_into(header, pdu, &mut out)?;
        Ok(out)
    }

    /// Decode an ADU carrying a response.
    ///
    /// # Errors
    ///
    /// Fails if the framing is malformed, the checksum does not match, or the
    /// PDU within it does not decode.
    fn decode_response(bytes: &[u8]) -> Result<(Self::Header, ResponsePdu)>;

    /// Encode a response into an ADU, appending to `out` (FR-R-140).
    ///
    /// The primitive, on the same terms as [`Self::encode_request_into`].
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode.
    fn encode_response_into(
        header: &Self::Header,
        pdu: &ResponsePdu,
        out: &mut Vec<u8>,
    ) -> Result<()>;

    /// Encode a response into an ADU, allocating a buffer for it (FR-R-140).
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode.
    fn encode_response(header: &Self::Header, pdu: &ResponsePdu) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        Self::encode_response_into(header, pdu, &mut out)?;
        Ok(out)
    }

    /// How the end of one of this framing's ADUs is determined (FR-R-122).
    fn boundary() -> AduBoundary;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::ascii::Ascii;
    use crate::frame::rtu::Rtu;
    use crate::frame::tcp::{MbapHeader, Tcp};
    use crate::frame::value::{Address, Quantity, RegisterValue, TransactionId, UnitId};
    use alloc::vec;

    /// A request every framing encodes without complaint.
    fn request() -> RequestPdu {
        RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        }
    }

    /// A request no framing can encode: 124 registers exceeds the 123 a Write
    /// Multiple Registers request may carry (FR-R-033).
    fn unencodable() -> RequestPdu {
        RequestPdu::WriteMultipleRegisters {
            address: Address(0),
            registers: vec![RegisterValue(0); 124],
        }
    }

    /// A response every framing encodes without complaint.
    fn response() -> ResponsePdu {
        ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(0x022B)],
        }
    }

    /// Assert FR-R-140, FR-R-141 and FR-R-142 over one framing.
    fn appending_encode_holds<F: Framing>(header: &F::Header) {
        let allocating = F::encode_request(header, &request()).expect("encodes");

        // FR-R-140 — appending yields the same bytes as allocating, after what
        // the buffer already held.
        let mut out = vec![0xAA, 0xBB];
        F::encode_request_into(header, &request(), &mut out).expect("encodes");
        assert_eq!(out.first(), Some(&0xAA));
        assert_eq!(out.get(1), Some(&0xBB));
        assert_eq!(out.get(2..), Some(allocating.as_slice()));

        // FR-R-140 — and the same holds for a response.
        let allocating = F::encode_response(header, &response()).expect("encodes");
        let mut out = Vec::new();
        F::encode_response_into(header, &response(), &mut out).expect("encodes");
        assert_eq!(out, allocating);

        // FR-R-141 — the framing's maximum is reserved before the first byte,
        // so no encode beneath it can reallocate the caller's buffer.
        let mut out = Vec::new();
        F::encode_request_into(header, &request(), &mut out).expect("encodes");
        assert!(
            out.capacity() >= F::MAX_ADU_LEN,
            "reserved {} of {}",
            out.capacity(),
            F::MAX_ADU_LEN
        );

        // FR-R-142 — a failure leaves the buffer exactly as it was found.
        let mut out = vec![0x01, 0x02, 0x03];
        F::encode_request_into(header, &unencodable(), &mut out).expect_err("rejects");
        assert_eq!(out, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    /// FR-R-144 — whether a framing locates the next boundary from the wire
    /// alone follows from its boundary rule, so a framing cannot state one rule
    /// and answer by another. Asserted through `boundary()` for that reason,
    /// rather than against a constant each framing could set independently.
    fn ut_boundary_self_locating_by_framing() {
        // Silence and delimiters are on the wire; the next frame is findable
        // without the one that failed.
        assert!(Rtu::boundary().is_self_locating());
        assert!(Ascii::boundary().is_self_locating());
        // A length field is carried by the frame that failed.
        assert!(!Tcp::boundary().is_self_locating());
    }

    #[test]
    /// FR-R-140, FR-R-141, FR-R-142 — RTU's appending encode appends the bytes
    /// its allocating form returns, reserves the framing maximum first, and
    /// restores the buffer when it fails.
    fn ut_rtu_appending_encode() {
        appending_encode_holds::<Rtu>(&UnitId(0x11));
    }

    #[test]
    /// FR-R-140, FR-R-141, FR-R-142 — TCP's appending encode does the same,
    /// with a length field that is only known once the PDU has been written.
    fn ut_tcp_appending_encode() {
        appending_encode_holds::<Tcp>(&MbapHeader {
            transaction_id: TransactionId(0x0001),
            unit_id: UnitId(0x11),
        });
    }

    #[test]
    /// FR-R-140, FR-R-141, FR-R-142 — ASCII's appending encode does the same,
    /// despite transforming the binary ADU through its scratch buffer
    /// (FR-R-143).
    fn ut_ascii_appending_encode() {
        appending_encode_holds::<Ascii>(&UnitId(0x11));
    }
}
