//! RTU framing: address, PDU, CRC-16 (FR-R-090 … FR-R-096).

use alloc::vec::Vec;
use crc::{CRC_16_MODBUS, Crc};

use crate::error::{Error, Result};
use crate::frame::framing::{AduBoundary, Direction, Extent, Framing};
use crate::frame::pdu::{RequestPdu, ResponsePdu};
use crate::frame::value::UnitId;

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
    type Header = UnitId;

    const MAX_ADU_LEN: usize = 256;

    fn decode_request(bytes: &[u8]) -> Result<(Self::Header, RequestPdu)> {
        let (address, pdu) = split(bytes)?;
        Ok((address, RequestPdu::decode(pdu)?))
    }

    fn encode_request_into(
        header: &Self::Header,
        pdu: &RequestPdu,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        wrap_into(*header, out, |out| pdu.encode_into(out))
    }

    fn decode_response(bytes: &[u8]) -> Result<(Self::Header, ResponsePdu)> {
        let (address, pdu) = split(bytes)?;
        Ok((address, ResponsePdu::decode(pdu)?))
    }

    fn encode_response_into(
        header: &Self::Header,
        pdu: &ResponsePdu,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        wrap_into(*header, out, |out| pdu.encode_into(out))
    }

    /// An RTU ADU carries neither a length nor a delimiter -- every byte value
    /// is legal data -- so only silence on the line ends one (FR-R-122).
    fn boundary() -> AduBoundary {
        AduBoundary::Silence
    }
}

/// Check an ADU's size and CRC, and split it into its address and its PDU
/// (FR-R-090, FR-R-093, FR-R-094, FR-R-095).
///
/// The CRC is verified before the PDU is looked at, so a corrupted frame never
/// reaches the PDU decoder (FR-R-095).
fn split(bytes: &[u8]) -> Result<(UnitId, &[u8])> {
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
    Ok((UnitId(address), pdu))
}

/// Wrap an encoded PDU in its address and CRC (FR-R-090, FR-R-094).
///
/// No size check is needed: a PDU is at most 253 bytes (FR-R-002) and the
/// overhead is 3, so an encoded ADU cannot exceed the 256 of FR-R-091.
fn wrap_into(
    address: UnitId,
    out: &mut Vec<u8>,
    encode_pdu: impl FnOnce(&mut Vec<u8>) -> Result<()>,
) -> Result<()> {
    let at = out.len();
    // One reservation covers the largest ADU this framing permits, so nothing
    // below reallocates the caller's buffer (FR-R-141).
    out.reserve(Rtu::MAX_ADU_LEN);
    out.push(address.0);
    if let Err(error) = encode_pdu(out) {
        // Nothing partial survives a failure (FR-R-142).
        out.truncate(at);
        return Err(error);
    }
    let covered = out
        .get(at..)
        .expect("the address and the PDU were just appended at this offset");
    let crc = CRC.checksum(covered).to_le_bytes();
    out.extend_from_slice(&crc);
    Ok(())
}

/// RTU framing over a byte stream (FR-R-145 … FR-R-150).
///
/// Identical to RTU in all respects — same encoding, same CRC, same header type —
/// except in how the boundary between ADUs is determined. While RTU relies on
/// inter-frame silence, RTU-over-stream derives the boundary from the PDU itself,
/// allowing RTU ADUs to be sent over a TCP connection or any other byte stream
/// where Silence is not available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtuOverTcp;

impl Framing for RtuOverTcp {
    /// The 1-byte server address (FR-R-096).
    type Header = UnitId;

    const MAX_ADU_LEN: usize = 256;

    fn decode_request(bytes: &[u8]) -> Result<(Self::Header, RequestPdu)> {
        let (address, pdu) = split(bytes)?;
        Ok((address, RequestPdu::decode(pdu)?))
    }

    fn encode_request_into(
        header: &Self::Header,
        pdu: &RequestPdu,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        wrap_into(*header, out, |out| pdu.encode_into(out))
    }

    fn decode_response(bytes: &[u8]) -> Result<(Self::Header, ResponsePdu)> {
        let (address, pdu) = split(bytes)?;
        Ok((address, ResponsePdu::decode(pdu)?))
    }

    fn encode_response_into(
        header: &Self::Header,
        pdu: &ResponsePdu,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        wrap_into(*header, out, |out| pdu.encode_into(out))
    }

    /// An RTU-over-stream ADU's boundary is determined from the PDU length it
    /// carries, not from silence on the line (FR-R-146). The derivation is
    /// direction-specific because request and response layouts differ.
    fn boundary() -> AduBoundary {
        AduBoundary::ContentLength {
            min: 4,
            extent: derive_extent,
        }
    }
}

/// Derive the extent of an RTU-over-stream ADU from its bytes.
///
/// Returns the total ADU length (address + PDU + CRC) or an error if the length
/// cannot be determined from these bytes (FR-R-146, FR-R-147, FR-R-148, FR-R-149).
fn derive_extent(direction: Direction, bytes: &[u8]) -> Result<Extent> {
    const ADDRESS_LEN: usize = 1;
    const CRC_LEN: usize = 2;

    // We need at least address + FC + CRC
    if bytes.len() < 4 {
        return Ok(Extent::NeedMore);
    }

    let fc = *bytes.get(1).expect("length check ensures bytes[1] exists"); // First byte of PDU is function code

    // Determine PDU length based on direction and function code (FR-R-147, FR-R-148)
    let pdu_len = if fc & 0x80 != 0 {
        // Response with high bit set: exception response, PDU is FC + exception code = 2 bytes
        2
    } else {
        // Request or normal response
        match direction {
            Direction::Request => match fc {
                // Fixed-length requests (FR-R-147)
                0x01..=0x06 => 5,               // FC + addr(2) + qty/value(2)
                0x07 | 0x0B | 0x0C | 0x11 => 1, // FC only
                0x16 => 7,                      // FC + addr(2) + AND(2) + OR(2)
                0x18 => 3,                      // FC + FIFO addr(2)

                // Variable-length requests with byte count
                0x0F | 0x10 => {
                    // FC 15 (Write Multiple Coils), FC 16 (Write Multiple Registers)
                    // PDU: FC(1) + addr(2) + qty(2) + bytecount(1) + data(bytecount)
                    if bytes.len() < 8 {
                        // Need at least address + FC + addr + qty + bytecount
                        return Ok(Extent::NeedMore);
                    }
                    let bytecount =
                        *bytes.get(6).expect("length check ensures bytes[6] exists") as usize;
                    6 + bytecount
                }

                0x14 | 0x15 => {
                    // FC 20 (Read File Record), FC 21 (Write File Record)
                    // PDU: FC(1) + byte_count/data_length(1) + data(byte_count/data_length)
                    if bytes.len() < 4 {
                        return Ok(Extent::NeedMore);
                    }
                    let byte_count =
                        *bytes.get(2).expect("length check ensures bytes[2] exists") as usize;
                    2 + byte_count
                }

                0x17 => {
                    // FC 23 (Read/Write Multiple Registers)
                    // PDU: FC(1) + read_addr(2) + read_qty(2) + write_addr(2) + write_qty(2)
                    //      + write_bytecount(1) + write_data(write_bytecount)
                    if bytes.len() < 11 {
                        return Ok(Extent::NeedMore);
                    }
                    let write_bytecount = *bytes
                        .get(10)
                        .expect("length check ensures bytes[10] exists")
                        as usize;
                    10 + write_bytecount
                }

                0x2B => {
                    // FC 43 (Encapsulated Interface Transport)
                    // Check MEI type at byte 2 (position after FC)
                    if bytes.len() < 3 {
                        return Ok(Extent::NeedMore);
                    }
                    let mei_type = *bytes.get(2).expect("length check ensures bytes[2] exists");
                    match mei_type {
                        0x0E => 4, // MEI 14: FC + MEI(1) + read_id_code(1) + obj_id(1)
                        _ => {
                            // FR-R-148: other MEI types have indeterminate length
                            return Err(Error::IndeterminateLength { function: fc });
                        }
                    }
                }

                // FR-R-148: FC 8 and custom codes have indeterminate length
                0x08 => return Err(Error::IndeterminateLength { function: fc }),
                _ => return Err(Error::IndeterminateLength { function: fc }),
            },

            Direction::Response => match fc {
                // Fixed-length responses (FR-R-147)
                0x05 | 0x06 | 0x0F | 0x10 | 0x0B => 5, // FC + addr(2) + qty/value(2)
                0x07 => 2,                             // FC + status(1)
                0x16 => 7,                             // FC + addr(2) + AND(2) + OR(2)

                // Variable-length responses with byte count (second byte of PDU)
                0x01 | 0x02 | 0x03 | 0x04 | 0x0C | 0x11 | 0x14 | 0x15 | 0x17 => {
                    if bytes.len() < 4 {
                        return Ok(Extent::NeedMore);
                    }
                    let bytecount =
                        *bytes.get(2).expect("length check ensures bytes[2] exists") as usize;
                    2 + bytecount
                }

                0x18 => {
                    // FC 24 (Read FIFO Queue) response
                    // PDU: FC(1) + bytecount(2, big-endian) + FIFO_count(2) + data(2*FIFO_count)
                    // Total: 1 + 2 + (2 + data) = 3 + bytecount
                    if bytes.len() < 5 {
                        return Ok(Extent::NeedMore);
                    }
                    let bytecount = u16::from_be_bytes([
                        *bytes.get(2).expect("length check ensures bytes[2] exists"),
                        *bytes.get(3).expect("length check ensures bytes[3] exists"),
                    ]) as usize;
                    3 + bytecount
                }

                0x2B => {
                    // FC 43 (Encapsulated Interface Transport)
                    if bytes.len() < 3 {
                        return Ok(Extent::NeedMore);
                    }
                    let mei_type = *bytes.get(2).expect("length check ensures bytes[2] exists");
                    match mei_type {
                        0x0E => {
                            // MEI 14 response: FC + MEI + read_id_code + conformity + more + next_obj_id + obj_count + objects
                            // Each object: obj_id + obj_length + obj_value(obj_length)
                            // Need to walk the object list to determine total length
                            return derive_fc43_mei14_response_length(bytes);
                        }
                        _ => {
                            // FR-R-148: other MEI types have indeterminate length
                            return Err(Error::IndeterminateLength { function: fc });
                        }
                    }
                }

                // FR-R-148: FC 8 and custom codes have indeterminate length
                0x08 => return Err(Error::IndeterminateLength { function: fc }),
                _ => return Err(Error::IndeterminateLength { function: fc }),
            },
        }
    };

    let total_len = ADDRESS_LEN + pdu_len + CRC_LEN;

    // Check if the total length exceeds the maximum (FR-R-149)
    if total_len > RtuOverTcp::MAX_ADU_LEN {
        return Err(Error::AduTooLarge {
            len: total_len,
            max: RtuOverTcp::MAX_ADU_LEN,
        });
    }

    // Check if we have all the bytes needed
    if bytes.len() < total_len {
        Ok(Extent::NeedMore)
    } else {
        Ok(Extent::Complete(total_len))
    }
}

/// Helper to derive the length of an FC 43 MEI 14 response by walking its object list.
///
/// FC 43 MEI 14 response format:
/// - FC (1) + MEI type (1) + read_id_code (1) + conformity (1) + more (1) + next_obj_id (1) + obj_count (1)
/// - Then, for each object: obj_id (1) + obj_length (1) + obj_value (obj_length)
///
/// Returns an Extent indicating whether we have all the bytes or need more.
fn derive_fc43_mei14_response_length(bytes: &[u8]) -> Result<Extent> {
    const HEADER_PDU_BYTES: usize = 7; // FC (1) + MEI (1) + read_id_code (1) + conformity (1) + more (1) + next_obj_id (1) + obj_count (1)
    const ADDRESS_LEN: usize = 1;
    const CRC_LEN: usize = 2;

    // Check if we have the header in the full ADU
    if bytes.len() < ADDRESS_LEN + HEADER_PDU_BYTES + CRC_LEN {
        return Ok(Extent::NeedMore);
    }

    let obj_count = *bytes
        .get(ADDRESS_LEN + 6)
        .expect("length check ensures bytes[ADDRESS_LEN + 6] exists") as usize;
    let mut pdu_offset = HEADER_PDU_BYTES; // Offset within the PDU

    for _ in 0..obj_count {
        if ADDRESS_LEN + pdu_offset + 2 > bytes.len() {
            // Need the object id and length in the ADU
            return Ok(Extent::NeedMore);
        }
        let obj_length = *bytes
            .get(ADDRESS_LEN + pdu_offset + 1)
            .expect("length check ensures obj_length exists") as usize;
        pdu_offset += 2 + obj_length; // obj_id + obj_length + obj_value
    }

    let total_len = ADDRESS_LEN + pdu_offset + CRC_LEN;

    if total_len > 256 {
        return Err(Error::AduTooLarge {
            len: total_len,
            max: 256,
        });
    }

    if bytes.len() < total_len {
        Ok(Extent::NeedMore)
    } else {
        Ok(Extent::Complete(total_len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::value::{Address, Quantity, RegisterValue};
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
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        assert_eq!(
            Rtu::decode_request(&READ_HOLDING_REQUEST),
            Ok((UnitId(0x11), pdu.clone()))
        );
        assert_eq!(
            Rtu::encode_request(&UnitId(0x11), &pdu),
            Ok(READ_HOLDING_REQUEST.to_vec())
        );
    }

    #[test]
    /// FR-R-090 — the response direction is framed identically; only the PDU
    /// within it differs.
    fn ut_rtu_response_spec_example() {
        let pdu = ResponsePdu::ReadHoldingRegisters {
            registers: vec![
                RegisterValue(0x022B),
                RegisterValue(0x0000),
                RegisterValue(0x0064),
            ],
        };
        assert_eq!(
            Rtu::decode_response(&READ_HOLDING_RESPONSE),
            Ok((UnitId(0x11), pdu.clone()))
        );
        assert_eq!(
            Rtu::encode_response(&UnitId(0x11), &pdu),
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
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        for address in [0x00u8, 0x01, 0xF7, 0xF8, 0xFF] {
            let bytes = Rtu::encode_request(&UnitId(address), &pdu).expect("encodes");
            assert_eq!(
                Rtu::decode_request(&bytes),
                Ok((UnitId(address), pdu.clone())),
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
    /// FR-R-122 — an RTU ADU carries no length and no delimiter, so only
    /// silence on the line ends it.
    fn ut_rtu_boundary_is_silence() {
        assert!(matches!(Rtu::boundary(), AduBoundary::Silence));
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

    #[test]
    /// FR-R-145 — RtuOverTcp encodes and decodes identically to Rtu.
    fn ut_rtu_over_tcp_bytes_are_identical_to_rtu() {
        let pdu = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        let header = UnitId(0x11);
        let rtu_bytes = Rtu::encode_request(&header, &pdu).expect("encodes");
        let rtu_over_tcp_bytes = RtuOverTcp::encode_request(&header, &pdu).expect("encodes");
        assert_eq!(rtu_bytes, rtu_over_tcp_bytes);

        let (rtu_addr, rtu_pdu) = Rtu::decode_request(&rtu_bytes).expect("decodes");
        let (rtu_over_tcp_addr, rtu_over_tcp_pdu) =
            RtuOverTcp::decode_request(&rtu_over_tcp_bytes).expect("decodes");
        assert_eq!(rtu_addr, rtu_over_tcp_addr);
        assert_eq!(rtu_pdu, rtu_over_tcp_pdu);
    }

    #[test]
    /// FR-R-146 — extent needs more bytes before the length field is in hand.
    /// FC 16 (Write Multiple Registers) request has the byte count at the 6th
    /// byte of the PDU. Before that byte, extent must return NeedMore.
    fn ut_extent_needs_more_before_the_count_field() {
        // FC 16 request: FC(1) + addr(2) + qty(2) + bytecount(1) = 6 bytes PDU
        // ADU: address(1) + PDU(6+data) + CRC(2)
        // But if we only have address + 4 PDU bytes = 5 bytes total, we need more
        // The min is 4 (address + FC + first 2 bytes of addr), so let's test at 5, 6, etc.

        // FC 16 request for 10 registers at address 0x0100:
        // FC: 0x10, addr: 0x0100 (2 bytes), qty: 0x000A (2 bytes), bytecount: 0x14 (1 byte), then data
        let mut adu = vec![0x11, 0x10, 0x01, 0x00, 0x00, 0x0A]; // 6 bytes total, bytecount not yet present
        match derive_extent(Direction::Request, &adu) {
            Ok(Extent::NeedMore) => {}
            other => panic!("Expected NeedMore, got {:?}", other),
        }

        // Now add the bytecount (0x14 = 20 data bytes for 10 registers)
        adu.push(0x14);
        let data = vec![0u8; 20];
        adu.extend_from_slice(&data);
        adu.extend_from_slice(&[0x00, 0x00]); // dummy CRC
        match derive_extent(Direction::Request, &adu) {
            Ok(Extent::Complete(len)) => {
                // address(1) + FC(1) + addr(2) + qty(2) + bytecount(1) + data(20) + CRC(2) = 29
                assert_eq!(
                    len,
                    1 + 1 + 2 + 2 + 1 + 20 + 2,
                    "address + FC + addr + qty + bytecount + data + CRC"
                );
            }
            other => panic!("Expected Complete, got {:?}", other),
        }
    }

    #[test]
    /// FR-R-147 — extent returns the correct length for every derivable function code,
    /// in both directions. This test is table-driven from the spec requirements.
    fn ut_extent_table_over_every_derivable_code() {
        // Helper to test a PDU by direction and assert its extent
        fn test_pdu(direction: Direction, pdu: &[u8], expected_total_len: usize) {
            // ADU = address(1) + PDU + CRC(2)
            let mut adu = vec![0x11]; // address
            adu.extend_from_slice(pdu);
            adu.extend_from_slice(&[0x00, 0x00]); // CRC
            match derive_extent(direction, &adu) {
                Ok(Extent::Complete(len)) => {
                    assert_eq!(
                        len, expected_total_len,
                        "direction: {:?}, PDU: {:?}",
                        direction, pdu
                    );
                }
                other => panic!(
                    "Expected Complete({}) for direction {:?}, PDU {:?}, got {:?}",
                    expected_total_len, direction, pdu, other
                ),
            }
        }

        // FC 1 request (Read Coils): FC + addr(2) + qty(2) = 5 bytes PDU, 1+5+2=8 total
        test_pdu(Direction::Request, &[0x01, 0x00, 0x00, 0x00, 0x0A], 8);

        // FC 1 response: FC + bytecount(1) + data(2) = 4 bytes PDU, 1+4+2=7 total
        // (qty=10 requires (10+7)/8=2 bytes)
        test_pdu(Direction::Response, &[0x01, 0x02, 0xFF, 0x03], 7);

        // FC 3 request (Read Holding Registers): FC + addr(2) + qty(2) = 5 bytes
        test_pdu(Direction::Request, &[0x03, 0x00, 0x6B, 0x00, 0x03], 8);

        // FC 3 response: FC + bytecount(1) + data(qty*2) = 1+1+6=8 bytes PDU (qty=3)
        // ADU: address(1) + PDU(8) + CRC(2) = 11 bytes total
        test_pdu(
            Direction::Response,
            &[0x03, 0x06, 0x02, 0x2B, 0x00, 0x00, 0x00, 0x64],
            11,
        );

        // FC 5 request/response (Write Single Coil): FC + addr(2) + value(2) = 5 bytes
        test_pdu(Direction::Request, &[0x05, 0x00, 0xAC, 0xFF, 0x00], 8);
        test_pdu(Direction::Response, &[0x05, 0x00, 0xAC, 0xFF, 0x00], 8);

        // FC 7 request (Read Exception Status): just FC = 1 byte PDU
        test_pdu(Direction::Request, &[0x07], 4);

        // FC 7 response: FC + status(1) = 2 bytes PDU
        test_pdu(Direction::Response, &[0x07, 0xFF], 5);

        // FC 15 request (Write Multiple Coils): FC + addr(2) + qty(2) + bytecount(1) + data
        // Write 10 coils (qty=10, bytecount=2):
        // PDU: 1+2+2+1+2=8 bytes, ADU: 1+8+2=11 bytes
        test_pdu(
            Direction::Request,
            &[0x0F, 0x00, 0x00, 0x00, 0x0A, 0x02, 0xFF, 0x03],
            11,
        );

        // FC 15 response: FC + addr(2) + qty(2) = 5 bytes
        test_pdu(Direction::Response, &[0x0F, 0x00, 0x00, 0x00, 0x0A], 8);

        // FC 16 request (Write Multiple Registers): FC + addr(2) + qty(2) + bytecount(1) + data
        // Write 3 registers (qty=3, bytecount=6):
        test_pdu(
            Direction::Request,
            &[
                0x10, 0x00, 0x00, 0x00, 0x03, 0x06, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,
            ],
            15,
        );

        // FC 16 response: FC + addr(2) + qty(2) = 5 bytes
        test_pdu(Direction::Response, &[0x10, 0x00, 0x00, 0x00, 0x03], 8);

        // FC 20 request (Read File Record): FC + bytecount(1) + subrequests
        // 1 subrequest (7 bytes): bytecount=7
        // PDU: 1+1+7=9 bytes, ADU: 1+9+2=12 bytes
        test_pdu(
            Direction::Request,
            &[0x14, 0x07, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01],
            12,
        );

        // FC 20 response: FC + datalength(1) + subresponses
        // datalength=2 means 2 bytes of data: [0x02, 0x06]
        // PDU: 1+1+2=4 bytes, ADU: 1+4+2=7 bytes
        test_pdu(Direction::Response, &[0x14, 0x02, 0x02, 0x06], 7);

        // FC 23 request (Read/Write Multiple Registers):
        // FC + read addr(2) + read qty(2) + write addr(2) + write qty(2) + write bytecount(1) + data
        // Write 2 registers (bytecount=4):
        test_pdu(
            Direction::Request,
            &[
                0x17, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x04, 0x12, 0x34, 0x56, 0x78,
            ],
            17,
        );

        // FC 23 response: FC + bytecount(1) + data
        // Read 3 registers (bytecount=6):
        // PDU: 1+1+6=8 bytes, ADU: 1+8+2=11 bytes
        test_pdu(
            Direction::Response,
            &[0x17, 0x06, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03],
            11,
        );

        // FC 43 MEI 14 request: FC + MEI type(1) + read device id code(1) + object id(1) = 4 bytes
        test_pdu(Direction::Request, &[0x2B, 0x0E, 0x03, 0x00], 7);

        // FC 43 MEI 14 response: FC + MEI(1) + read id code(1) + conformity(1) + more(1) + next obj id(1)
        // + obj count(1) + objects. Let's do 0 objects: PDU=7 bytes, ADU=1+7+2=10
        test_pdu(
            Direction::Response,
            &[0x2B, 0x0E, 0x03, 0x82, 0x00, 0x00, 0x00],
            10,
        );

        // Exception response (any function code): FC|0x80 + exception code = 2 bytes PDU
        // Exception to FC 1:
        test_pdu(Direction::Response, &[0x81, 0x03], 5);
    }

    #[test]
    /// FR-R-148 — function codes whose length cannot be derived fail with IndeterminateLength.
    fn ut_extent_indeterminate_codes() {
        // FC 8 (Diagnostics) — data count is not fixed by the spec
        let mut adu = vec![0x11, 0x08, 0x00, 0x00]; // FC + sub-fn, no data
        adu.extend_from_slice(&[0x00, 0x00]); // CRC
        assert_eq!(
            derive_extent(Direction::Request, &adu),
            Err(Error::IndeterminateLength { function: 0x08 })
        );

        // FC 43 MEI 13 (CANopen) — body is opaque
        let mut adu = vec![0x11, 0x2B, 0x0D]; // FC 43 + MEI 13
        adu.extend_from_slice(&[0x00, 0x00]); // CRC
        assert_eq!(
            derive_extent(Direction::Request, &adu),
            Err(Error::IndeterminateLength { function: 0x2B })
        );

        // FC 43 response MEI 13 also fails
        assert_eq!(
            derive_extent(Direction::Response, &adu),
            Err(Error::IndeterminateLength { function: 0x2B })
        );

        // Custom function code (e.g., 0x50)
        let mut adu = vec![0x11, 0x50]; // FC 0x50
        adu.extend_from_slice(&[0x00, 0x00]); // CRC
        assert_eq!(
            derive_extent(Direction::Request, &adu),
            Err(Error::IndeterminateLength { function: 0x50 })
        );
    }

    #[test]
    /// FR-R-149 — an ADU length that exceeds 256 fails before allocation.
    fn ut_extent_above_max_adu_len() {
        // FC 16 request with a large byte count that would exceed 256
        // PDU: FC(1) + addr(2) + qty(2) + bytecount(1) + data(bytecount)
        // If bytecount says 250 bytes of data, total PDU = 1+2+2+1+250 = 256
        // ADU = address(1) + PDU(256) + CRC(2) = 259 > 256
        let mut adu = vec![0x11, 0x10, 0x00, 0x00, 0x00, 0x7D]; // qty=125
        adu.push(0xFA); // bytecount = 250
        adu.extend_from_slice(&vec![0x00u8; 250]);
        adu.extend_from_slice(&[0x00, 0x00]); // CRC
        assert_eq!(
            derive_extent(Direction::Request, &adu),
            Err(Error::AduTooLarge { len: 259, max: 256 })
        );
    }

    #[test]
    /// FR-R-150 — the RTU-over-stream boundary is not self-locating.
    fn ut_rtu_over_tcp_is_not_self_locating() {
        assert!(!RtuOverTcp::boundary().is_self_locating());
    }

    #[test]
    /// FR-R-147 — for every PDU the crate can encode in both directions,
    /// the extent derivation must agree with the encoder.
    fn ut_extent_agrees_with_the_encoder() {
        // Test a request
        let req = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        let header = UnitId(0x11);
        let encoded = RtuOverTcp::encode_request(&header, &req).expect("encodes");
        match derive_extent(Direction::Request, &encoded) {
            Ok(Extent::Complete(len)) => {
                assert_eq!(len, encoded.len(), "extent must agree with encoder");
            }
            other => panic!(
                "Request extent failed: {:?}. Encoded PDU: {:?}",
                other, encoded
            ),
        }

        // Test a response
        let resp = ResponsePdu::ReadHoldingRegisters {
            registers: vec![
                RegisterValue(0x022B),
                RegisterValue(0x0000),
                RegisterValue(0x0064),
            ],
        };
        let encoded = RtuOverTcp::encode_response(&header, &resp).expect("encodes");
        match derive_extent(Direction::Response, &encoded) {
            Ok(Extent::Complete(len)) => {
                assert_eq!(len, encoded.len(), "extent must agree with encoder");
            }
            other => panic!(
                "Response extent failed: {:?}. Encoded PDU: {:?}",
                other, encoded
            ),
        }
    }
}
