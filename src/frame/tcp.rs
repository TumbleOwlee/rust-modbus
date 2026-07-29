//! TCP framing: the MBAP header (FR-R-100 … FR-R-106).

use alloc::vec::Vec;

use crate::error::{Error, Result};
use crate::frame::framing::{AduBoundary, Framing};
use crate::frame::pdu::{RequestPdu, ResponsePdu};
use crate::frame::value::{TransactionId, UnitId};

/// What identifies a peer and a transaction over TCP (FR-R-101).
///
/// The protocol identifier is not a field here: FR-R-102 fixes it at 0, so
/// carrying it would let a caller construct a header the encoder must reject.
/// The length field is not a field either — FR-R-103 derives it from the PDU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MbapHeader {
    /// Echoed back by the server, so a client can match a response to the
    /// request it answers.
    pub transaction_id: TransactionId,
    /// Identifies the target device behind a gateway; the TCP analogue of the
    /// RTU address.
    pub unit_id: UnitId,
}

/// TCP framing (FR-R-100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tcp;

/// Bytes in an MBAP header (FR-R-100).
const HEADER_LEN: usize = 7;

/// Bytes the length field covers besides the PDU: the unit identifier
/// (FR-R-103).
const LENGTH_OVERHEAD: usize = 1;

/// The value FR-R-102 fixes the protocol identifier at.
const PROTOCOL_MODBUS: u16 = 0;

/// Bounds on the MBAP length field: one unit identifier, and a PDU of 1 to 253
/// bytes (FR-R-105).
const MIN_LENGTH: u32 = 1;
/// Bounds on the MBAP length field (FR-R-105).
const MAX_LENGTH: u32 = 254;

impl Framing for Tcp {
    type Header = MbapHeader;

    const MAX_ADU_LEN: usize = 260;

    fn decode_request(bytes: &[u8]) -> Result<(Self::Header, RequestPdu)> {
        let (header, pdu) = split(bytes)?;
        Ok((header, RequestPdu::decode(pdu)?))
    }

    fn encode_request(header: &Self::Header, pdu: &RequestPdu) -> Result<Vec<u8>> {
        wrap(header, &pdu.encode()?)
    }

    fn decode_response(bytes: &[u8]) -> Result<(Self::Header, ResponsePdu)> {
        let (header, pdu) = split(bytes)?;
        Ok((header, ResponsePdu::decode(pdu)?))
    }

    fn encode_response(header: &Self::Header, pdu: &ResponsePdu) -> Result<Vec<u8>> {
        wrap(header, &pdu.encode()?)
    }

    /// The MBAP length field gives the rest of the ADU, so six bytes are enough
    /// to know how long the whole of it is (FR-R-122).
    fn boundary() -> AduBoundary {
        AduBoundary::Prefixed {
            prefix: LENGTH_PREFIX_LEN,
            total: adu_len,
        }
    }
}

/// Bytes needed before the MBAP length field has been read in full.
const LENGTH_PREFIX_LEN: usize = 6;

/// The whole ADU's length, given the first [`LENGTH_PREFIX_LEN`] bytes of it
/// (FR-R-122).
///
/// The length field is validated against FR-R-105 first, so a hostile value can
/// never size a read.
fn adu_len(prefix: &[u8]) -> Result<usize> {
    let raw = prefix
        .get(4..LENGTH_PREFIX_LEN)
        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
        .ok_or(Error::Truncated {
            expected: LENGTH_PREFIX_LEN,
            supplied: prefix.len(),
        })?;
    let length = u32::from(u16::from_be_bytes(raw));
    check_length(length)?;
    Ok(LENGTH_PREFIX_LEN.saturating_add(usize::try_from(length).unwrap_or(usize::MAX)))
}

/// Reject an MBAP length field outside the range FR-R-105 permits.
fn check_length(length: u32) -> Result<()> {
    if !(MIN_LENGTH..=MAX_LENGTH).contains(&length) {
        return Err(Error::OutOfRange {
            field: "MBAP length",
            value: length,
            min: MIN_LENGTH,
            max: MAX_LENGTH,
        });
    }
    Ok(())
}

/// Validate an MBAP header and split the ADU into that header and its PDU
/// (FR-R-100 … FR-R-106).
///
/// The length field is range-checked before it is used for anything, so a
/// hostile value can never size a read or an allocation (FR-R-105).
fn split(bytes: &[u8]) -> Result<(MbapHeader, &[u8])> {
    if bytes.len() > Tcp::MAX_ADU_LEN {
        return Err(Error::AduTooLarge {
            len: bytes.len(),
            max: Tcp::MAX_ADU_LEN,
        });
    }
    let minimum = HEADER_LEN.saturating_add(1);
    if bytes.len() < minimum {
        return Err(Error::Truncated {
            expected: minimum,
            supplied: bytes.len(),
        });
    }
    let (head, pdu) = bytes
        .split_at_checked(HEADER_LEN)
        .expect("the length check above proved at least HEADER_LEN + 1 bytes");
    let head: [u8; HEADER_LEN] = head
        .try_into()
        .expect("splitting at HEADER_LEN yields exactly that many bytes");

    let protocol = u16::from_be_bytes([head[2], head[3]]);
    if protocol != PROTOCOL_MODBUS {
        return Err(Error::ProtocolIdentifier(protocol));
    }

    let length = u32::from(u16::from_be_bytes([head[4], head[5]]));
    check_length(length)?;

    // The length counts the unit identifier as well as the PDU (FR-R-103).
    let supplied = pdu.len().saturating_add(LENGTH_OVERHEAD);
    let expected = usize::try_from(length).unwrap_or(usize::MAX);
    if expected != supplied {
        return Err(Error::InvalidLength {
            expected,
            actual: supplied,
        });
    }

    Ok((
        MbapHeader {
            transaction_id: TransactionId(u16::from_be_bytes([head[0], head[1]])),
            unit_id: UnitId(head[6]),
        },
        pdu,
    ))
}

/// Prepend an MBAP header to an encoded PDU (FR-R-100, FR-R-103).
///
/// The length field is computed here rather than carried in [`MbapHeader`], so
/// it cannot disagree with the PDU it describes.
fn wrap(header: &MbapHeader, pdu: &[u8]) -> Result<Vec<u8>> {
    let length = pdu.len().saturating_add(LENGTH_OVERHEAD);
    let length = u16::try_from(length).unwrap_or(u16::MAX);
    if !(MIN_LENGTH..=MAX_LENGTH).contains(&u32::from(length)) {
        return Err(Error::OutOfRange {
            field: "MBAP length",
            value: u32::from(length),
            min: MIN_LENGTH,
            max: MAX_LENGTH,
        });
    }
    let mut bytes = Vec::with_capacity(pdu.len().saturating_add(HEADER_LEN));
    bytes.extend_from_slice(&header.transaction_id.0.to_be_bytes());
    bytes.extend_from_slice(&PROTOCOL_MODBUS.to_be_bytes());
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.push(header.unit_id.0);
    bytes.extend_from_slice(pdu);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::value::{Address, Quantity, RegisterValue};
    use alloc::vec;

    /// A Read Holding Registers request to unit `0x11`, transaction 1. The
    /// bytes follow the MBAP layout FR-R-101 fixes; the length field of 6 is
    /// the 5-byte PDU plus the unit identifier (FR-R-103).
    const READ_HOLDING_REQUEST: [u8; 12] = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x11, 0x03, 0x00, 0x6B, 0x00, 0x03,
    ];

    fn header() -> MbapHeader {
        MbapHeader {
            transaction_id: TransactionId(0x0001),
            unit_id: UnitId(0x11),
        }
    }

    #[test]
    /// FR-R-100, FR-R-101, FR-R-103 — a 7-byte MBAP header then the PDU, with
    /// the length field counting the unit identifier and the PDU.
    fn ut_tcp_request_spec_example() {
        let pdu = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        assert_eq!(
            Tcp::decode_request(&READ_HOLDING_REQUEST),
            Ok((header(), pdu.clone()))
        );
        assert_eq!(
            Tcp::encode_request(&header(), &pdu),
            Ok(READ_HOLDING_REQUEST.to_vec())
        );
    }

    #[test]
    /// FR-R-103 — the length field is derived from the PDU, never taken from
    /// the caller: an 8-byte response PDU yields a length of 9.
    fn ut_tcp_response_length_is_derived() {
        let pdu = ResponsePdu::ReadHoldingRegisters {
            registers: vec![
                RegisterValue(0x022B),
                RegisterValue(0x0000),
                RegisterValue(0x0064),
            ],
        };
        let bytes = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x09, 0x11, 0x03, 0x06, 0x02, 0x2B, 0x00, 0x00, 0x00,
            0x64,
        ];
        assert_eq!(Tcp::decode_response(&bytes), Ok((header(), pdu.clone())));
        assert_eq!(Tcp::encode_response(&header(), &pdu), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-102 — the protocol identifier is 0; anything else is not Modbus.
    fn ut_mbap_protocol_identifier_must_be_zero() {
        let mut bytes = READ_HOLDING_REQUEST;
        bytes[3] = 0x01;
        assert_eq!(
            Tcp::decode_request(&bytes),
            Err(Error::ProtocolIdentifier(1))
        );
    }

    #[test]
    /// FR-R-105 — a length field of 0, or above 254, cannot describe a PDU and
    /// is rejected before anything is sized by it.
    fn ut_mbap_length_out_of_range() {
        for (raw, value) in [(0x0000u16, 0u32), (0x00FF, 255), (0xFFFF, 65535)] {
            let mut bytes = READ_HOLDING_REQUEST;
            bytes[4] = u8::try_from(raw >> 8).expect("high byte");
            bytes[5] = u8::try_from(raw & 0xFF).expect("low byte");
            assert_eq!(
                Tcp::decode_request(&bytes),
                Err(Error::OutOfRange {
                    field: "MBAP length",
                    value,
                    min: 1,
                    max: 254,
                }),
                "length {raw:#06x}"
            );
        }
    }

    #[test]
    /// FR-R-106 — a length field that disagrees with the bytes actually
    /// supplied is an error, not a reason to trust one over the other.
    fn ut_mbap_length_must_match_supplied() {
        let mut bytes = READ_HOLDING_REQUEST;
        bytes[5] = 0x07;
        assert_eq!(
            Tcp::decode_request(&bytes),
            Err(Error::InvalidLength {
                expected: 7,
                actual: 6,
            })
        );
    }

    #[test]
    /// FR-R-104 — a TCP ADU is at most 260 bytes: 7 of header and 253 of PDU.
    fn ut_tcp_adu_too_large() {
        let bytes = vec![0x00; 261];
        assert_eq!(
            Tcp::decode_request(&bytes),
            Err(Error::AduTooLarge { len: 261, max: 260 })
        );
    }

    #[test]
    /// FR-R-122 — a TCP ADU's length comes from its MBAP length field: six
    /// bytes are enough to know the whole ADU's size.
    fn ut_tcp_boundary_is_prefixed() {
        let AduBoundary::Prefixed { prefix, total } = Tcp::boundary() else {
            panic!("TCP is length-prefixed");
        };
        assert_eq!(prefix, 6);
        // The length field of 6 covers the unit identifier and a 5-byte PDU,
        // so the ADU is the 6-byte prefix plus those 6 bytes.
        assert_eq!(total(&READ_HOLDING_REQUEST[..6]), Ok(12));
    }

    #[test]
    /// FR-R-122, FR-R-105 — the length field is validated before it sizes
    /// anything, so a boundary can never be computed from a bad one.
    fn ut_tcp_boundary_rejects_an_invalid_length() {
        let AduBoundary::Prefixed { total, .. } = Tcp::boundary() else {
            panic!("TCP is length-prefixed");
        };
        assert_eq!(
            total(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00]),
            Err(Error::OutOfRange {
                field: "MBAP length",
                value: 0,
                min: 1,
                max: 254,
            })
        );
    }

    #[test]
    /// FR-R-131 — an ADU too short to hold the header and a one-byte PDU names
    /// what it expected and what it got.
    fn ut_tcp_adu_too_short() {
        assert_eq!(
            Tcp::decode_request(&[0x00, 0x01, 0x00, 0x00, 0x00]),
            Err(Error::Truncated {
                expected: 8,
                supplied: 5,
            })
        );
    }
}
