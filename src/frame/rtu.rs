//! RTU framing: address, PDU, CRC-16 (FR-R-090 … FR-R-096).

use alloc::vec::Vec;
use crc::{CRC_16_MODBUS, Crc};

use crate::error::{Error, Result};
use crate::frame::framing::Framing;
use crate::frame::pdu::{RequestPdu, ResponsePdu};

/// RTU framing (FR-R-090).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rtu;

/// Bytes an RTU ADU always carries besides its PDU: the address and the CRC.
const OVERHEAD: usize = 3;

/// CRC-16 with the reversed polynomial `0xA001` and initial value `0xFFFF`
/// (FR-R-092), which is what `CRC_16_MODBUS` names.
const CRC: Crc<u16> = Crc::<u16>::new(&CRC_16_MODBUS);

impl Framing for Rtu {
    /// The 1-byte server address (FR-R-096).
    type Header = u8;

    const MAX_ADU_LEN: usize = 256;

    fn decode_request(bytes: &[u8]) -> Result<(Self::Header, RequestPdu)> {
        let (address, pdu) = split(bytes)?;
        Ok((address, RequestPdu::decode(pdu)?))
    }

    fn encode_request(header: &Self::Header, pdu: &RequestPdu) -> Result<Vec<u8>> {
        Ok(wrap(*header, &pdu.encode()?))
    }

    fn decode_response(bytes: &[u8]) -> Result<(Self::Header, ResponsePdu)> {
        let (address, pdu) = split(bytes)?;
        Ok((address, ResponsePdu::decode(pdu)?))
    }

    fn encode_response(header: &Self::Header, pdu: &ResponsePdu) -> Result<Vec<u8>> {
        Ok(wrap(*header, &pdu.encode()?))
    }
}

/// Check an ADU's size and CRC, and split it into its address and its PDU
/// (FR-R-090, FR-R-093, FR-R-094, FR-R-095).
///
/// The CRC is verified before the PDU is looked at, so a corrupted frame never
/// reaches the PDU decoder (FR-R-095).
fn split(bytes: &[u8]) -> Result<(u8, &[u8])> {
    if bytes.len() > Rtu::MAX_ADU_LEN {
        return Err(Error::AduTooLarge {
            len: bytes.len(),
            max: Rtu::MAX_ADU_LEN,
        });
    }
    // An address, a one-byte PDU, and the CRC are the least an ADU can be.
    let minimum = OVERHEAD.saturating_add(1);
    if bytes.len() < minimum {
        return Err(Error::Truncated {
            expected: minimum,
            supplied: bytes.len(),
        });
    }
    let (body, crc) = bytes
        .split_at_checked(bytes.len().saturating_sub(2))
        .expect("len - 2 is in bounds, since the length check above proved len >= 4");
    let (&address, pdu) = body
        .split_first()
        .expect("body is len - 2 >= 2 bytes, so it holds an address and a PDU");
    let actual = u16::from_le_bytes(
        crc.try_into()
            .expect("splitting at len - 2 leaves exactly two bytes"),
    );
    let expected = CRC.checksum(body);
    if expected != actual {
        return Err(Error::Checksum { expected, actual });
    }
    Ok((address, pdu))
}

/// Wrap an encoded PDU in its address and CRC (FR-R-090, FR-R-094).
///
/// No size check is needed: a PDU is at most 253 bytes (FR-R-002) and the
/// overhead is 3, so an encoded ADU cannot exceed the 256 of FR-R-091.
fn wrap(address: u8, pdu: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(pdu.len().saturating_add(OVERHEAD));
    bytes.push(address);
    bytes.extend_from_slice(pdu);
    bytes.extend_from_slice(&CRC.checksum(&bytes).to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// The specification's Read Holding Registers request to server `0x11`,
    /// with the CRC the polynomial in FR-R-092 produces over it.
    const READ_HOLDING_REQUEST: [u8; 8] = [0x11, 0x03, 0x00, 0x6B, 0x00, 0x03, 0x76, 0x87];

    /// Its response: three registers, same server.
    const READ_HOLDING_RESPONSE: [u8; 11] = [
        0x11, 0x03, 0x06, 0x02, 0x2B, 0x00, 0x00, 0x00, 0x64, 0xC8, 0xBA,
    ];

    #[test]
    /// FR-R-090, FR-R-093 — an RTU ADU is address, PDU, CRC, and the CRC covers
    /// the address and the whole PDU.
    fn ut_rtu_request_spec_example() {
        let pdu = RequestPdu::ReadHoldingRegisters {
            address: 0x006B,
            quantity: 3,
        };
        assert_eq!(
            Rtu::decode_request(&READ_HOLDING_REQUEST),
            Ok((0x11, pdu.clone()))
        );
        assert_eq!(
            Rtu::encode_request(&0x11, &pdu),
            Ok(READ_HOLDING_REQUEST.to_vec())
        );
    }

    #[test]
    /// FR-R-090 — the response direction is framed identically; only the PDU
    /// within it differs.
    fn ut_rtu_response_spec_example() {
        let pdu = ResponsePdu::ReadHoldingRegisters {
            registers: vec![0x022B, 0x0000, 0x0064],
        };
        assert_eq!(
            Rtu::decode_response(&READ_HOLDING_RESPONSE),
            Ok((0x11, pdu.clone()))
        );
        assert_eq!(
            Rtu::encode_response(&0x11, &pdu),
            Ok(READ_HOLDING_RESPONSE.to_vec())
        );
    }

    #[test]
    /// FR-R-095 — a CRC that does not match the bytes before it fails, and the
    /// PDU is not decoded.
    fn ut_rtu_crc_mismatch_rejected() {
        let mut bytes = READ_HOLDING_REQUEST;
        bytes[7] ^= 0xFF;
        assert_eq!(
            Rtu::decode_request(&bytes),
            Err(Error::Checksum {
                expected: 0x8776,
                actual: 0x7876,
            })
        );
    }

    #[test]
    /// FR-R-094 — the CRC goes out low byte first; swapping the two bytes is a
    /// mismatch, not an accepted alternative order.
    fn ut_rtu_crc_is_low_byte_first() {
        let mut bytes = READ_HOLDING_REQUEST;
        bytes.swap(6, 7);
        assert_eq!(
            Rtu::decode_request(&bytes),
            Err(Error::Checksum {
                expected: 0x8776,
                actual: 0x7687,
            })
        );
    }

    #[test]
    /// FR-R-096 — every 8-bit address decodes: 0 is broadcast, 1–247 address a
    /// server, and 248–255 are left for the caller to judge.
    fn ut_rtu_every_address_decodes() {
        let pdu = RequestPdu::ReadHoldingRegisters {
            address: 0x006B,
            quantity: 3,
        };
        for address in [0x00u8, 0x01, 0xF7, 0xF8, 0xFF] {
            let bytes = Rtu::encode_request(&address, &pdu).expect("encodes");
            assert_eq!(
                Rtu::decode_request(&bytes),
                Ok((address, pdu.clone())),
                "address {address:#04x}"
            );
        }
    }

    #[test]
    /// FR-R-091 — an ADU longer than 256 bytes is rejected on its size, before
    /// its CRC is computed or its PDU touched.
    fn ut_rtu_adu_too_large() {
        let bytes = vec![0x11; 257];
        assert_eq!(
            Rtu::decode_request(&bytes),
            Err(Error::AduTooLarge { len: 257, max: 256 })
        );
    }

    #[test]
    /// FR-R-131 — an ADU too short to hold an address, a one-byte PDU, and a
    /// CRC names what it expected and what it got.
    fn ut_rtu_adu_too_short() {
        assert_eq!(
            Rtu::decode_request(&[0x11, 0x03]),
            Err(Error::Truncated {
                expected: 4,
                supplied: 2,
            })
        );
    }
}
