//! ASCII framing: `:`, hexadecimal pairs, LRC, CR LF (FR-R-110 … FR-R-119).

use alloc::vec::Vec;

use crate::error::{Error, Result};
use crate::frame::framing::Framing;
use crate::frame::pdu::{RequestPdu, ResponsePdu};

/// ASCII framing (FR-R-110).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ascii;

/// Start character (FR-R-110).
const START: u8 = b':';

/// Terminator, strictly CR LF (FR-R-110).
const TERMINATOR: [u8; 2] = [0x0D, 0x0A];

/// Characters an ADU always carries besides its PDU: the start character, two
/// for the address, two for the LRC, and the two-character terminator.
const OVERHEAD: usize = 7;

impl Framing for Ascii {
    /// The 1-byte server address, with the same semantics as RTU's
    /// (FR-R-117).
    type Header = u8;

    const MAX_ADU_LEN: usize = 513;

    fn decode_request(bytes: &[u8]) -> Result<(Self::Header, RequestPdu)> {
        let (address, pdu) = split(bytes)?;
        Ok((address, RequestPdu::decode(&pdu)?))
    }

    fn encode_request(header: &Self::Header, pdu: &RequestPdu) -> Result<Vec<u8>> {
        Ok(wrap(*header, &pdu.encode()?))
    }

    fn decode_response(bytes: &[u8]) -> Result<(Self::Header, ResponsePdu)> {
        let (address, pdu) = split(bytes)?;
        Ok((address, ResponsePdu::decode(&pdu)?))
    }

    fn encode_response(header: &Self::Header, pdu: &ResponsePdu) -> Result<Vec<u8>> {
        Ok(wrap(*header, &pdu.encode()?))
    }
}

/// Unframe an ASCII ADU into its address and its decoded PDU bytes
/// (FR-R-110 … FR-R-116).
///
/// The LRC is verified before the PDU is looked at, so a corrupted frame never
/// reaches the PDU decoder (FR-R-115).
fn split(bytes: &[u8]) -> Result<(u8, Vec<u8>)> {
    if bytes.len() > Ascii::MAX_ADU_LEN {
        return Err(Error::AduTooLarge {
            len: bytes.len(),
            max: Ascii::MAX_ADU_LEN,
        });
    }
    // Start, two characters each for address, one PDU byte and the LRC, and
    // the terminator.
    let minimum = OVERHEAD.saturating_add(2);
    if bytes.len() < minimum {
        return Err(Error::Truncated {
            expected: minimum,
            supplied: bytes.len(),
        });
    }

    let Some((&first, rest)) = bytes.split_first() else {
        return Err(Error::Framing {
            element: "start character",
        });
    };
    if first != START {
        return Err(Error::Framing {
            element: "start character",
        });
    }
    let (body, terminator) = rest
        .split_at_checked(rest.len().saturating_sub(TERMINATOR.len()))
        .expect("the length check above proved rest holds more than a terminator");
    if terminator != TERMINATOR {
        return Err(Error::Framing {
            element: "terminator",
        });
    }
    if body.len() % 2 != 0 {
        return Err(Error::Framing {
            element: "hexadecimal character count",
        });
    }

    let decoded = decode_hex(body)?;
    let Some((&actual, covered)) = decoded.split_last() else {
        return Err(Error::Framing {
            element: "hexadecimal character count",
        });
    };
    let expected = lrc(covered);
    if expected != actual {
        return Err(Error::Checksum {
            expected: u16::from(expected),
            actual: u16::from(actual),
        });
    }

    let Some((&address, pdu)) = covered.split_first() else {
        return Err(Error::Truncated {
            expected: minimum,
            supplied: bytes.len(),
        });
    };
    Ok((address, pdu.to_vec()))
}

/// Frame an encoded PDU as an ASCII ADU (FR-R-110, FR-R-111, FR-R-114).
///
/// No size check is needed: a PDU is at most 253 bytes (FR-R-002), which with
/// the address and LRC is 255 encoded bytes, so 510 characters plus the 3 of
/// overhead is exactly the 513 of FR-R-113.
fn wrap(address: u8, pdu: &[u8]) -> Vec<u8> {
    let mut covered = Vec::with_capacity(pdu.len().saturating_add(1));
    covered.push(address);
    covered.extend_from_slice(pdu);

    let mut bytes = Vec::with_capacity(covered.len().saturating_mul(2).saturating_add(OVERHEAD));
    bytes.push(START);
    for byte in &covered {
        encode_hex(*byte, &mut bytes);
    }
    encode_hex(lrc(&covered), &mut bytes);
    bytes.extend_from_slice(&TERMINATOR);
    bytes
}

/// The LRC: the two's complement of the 8-bit sum of the decoded bytes
/// (FR-R-114).
fn lrc(bytes: &[u8]) -> u8 {
    bytes
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
        .wrapping_neg()
}

/// Decode hexadecimal character pairs into bytes (FR-R-111, FR-R-112).
fn decode_hex(chars: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks_exact(2) {
        let (high, low) = match *pair {
            [high, low] => (high, low),
            // `chunks_exact(2)` yields nothing else.
            _ => return Err(Error::Malformed),
        };
        bytes.push((nibble(high)? << 4) | nibble(low)?);
    }
    Ok(bytes)
}

/// One hexadecimal character, either case (FR-R-112).
fn nibble(character: u8) -> Result<u8> {
    match character {
        b'0'..=b'9' => Ok(character - b'0'),
        b'A'..=b'F' => Ok(character - b'A' + 10),
        b'a'..=b'f' => Ok(character - b'a' + 10),
        other => Err(Error::InvalidCharacter(other)),
    }
}

/// Append a byte as two uppercase hexadecimal characters, most significant
/// nibble first (FR-R-111, FR-R-112).
fn encode_hex(byte: u8, out: &mut Vec<u8>) {
    out.push(hex_digit(byte >> 4));
    out.push(hex_digit(byte & 0x0F));
}

/// One uppercase hexadecimal character. The argument is a nibble by
/// construction, so no case is unreachable.
fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0'.saturating_add(nibble),
        _ => b'A'.saturating_add(nibble).saturating_sub(10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::exception::{ExceptionCode, ExceptionResponse};
    use crate::frame::function::FunctionCode;
    use crate::frame::rtu::Rtu;
    use alloc::vec;

    /// The specification's Read Holding Registers request to server `0x11`,
    /// with the LRC the rule in FR-R-114 produces over its decoded bytes.
    const READ_HOLDING_REQUEST: &[u8] = b":1103006B00037E\r\n";

    fn read_holding() -> RequestPdu {
        RequestPdu::ReadHoldingRegisters {
            address: 0x006B,
            quantity: 3,
        }
    }

    #[test]
    /// FR-R-110, FR-R-111, FR-R-114 — start character, hexadecimal pairs most
    /// significant nibble first, LRC, CR LF.
    fn ut_ascii_request_spec_example() {
        assert_eq!(
            Ascii::decode_request(READ_HOLDING_REQUEST),
            Ok((0x11, read_holding()))
        );
        assert_eq!(
            Ascii::encode_request(&0x11, &read_holding()),
            Ok(READ_HOLDING_REQUEST.to_vec())
        );
    }

    #[test]
    /// FR-R-110 — the response direction is framed identically.
    fn ut_ascii_response_spec_example() {
        let bytes = b":110306022B0000006455\r\n";
        let pdu = ResponsePdu::ReadHoldingRegisters {
            registers: vec![0x022B, 0x0000, 0x0064],
        };
        assert_eq!(Ascii::decode_response(bytes), Ok((0x11, pdu.clone())));
        assert_eq!(Ascii::encode_response(&0x11, &pdu), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-112, FR-R-119 — lowercase input decodes, and re-encodes to the
    /// uppercase form of the same ADU.
    fn ut_ascii_lowercase_reencodes_uppercase() {
        let lowercase = b":1103006b00037e\r\n";
        let (address, pdu) = Ascii::decode_request(lowercase).expect("lowercase decodes");
        assert_eq!((address, pdu.clone()), (0x11, read_holding()));

        // The re-encoding is of what was decoded, not of a literal, so the
        // chain FR-R-119 describes is the one under test.
        let uppercase = Ascii::encode_request(&address, &pdu).expect("re-encodes");
        assert_eq!(uppercase, READ_HOLDING_REQUEST.to_vec());
        assert_eq!(Ascii::decode_request(&uppercase), Ok((address, pdu)));
    }

    #[test]
    /// FR-R-118 — ASCII is a framing choice, not a capability subset: an
    /// exception response frames like any other PDU.
    fn ut_ascii_carries_exception_responses() {
        let pdu = ResponsePdu::Exception(ExceptionResponse {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        });
        let bytes = Ascii::encode_response(&0x11, &pdu).expect("encodes");
        assert_eq!(bytes, b":1183026A\r\n".to_vec());
        assert_eq!(Ascii::decode_response(&bytes), Ok((0x11, pdu)));
    }

    #[test]
    /// FR-R-114, FR-R-115 — the LRC covers the decoded bytes, so an ADU whose
    /// LRC does not match them fails and the PDU is not decoded.
    fn ut_ascii_lrc_mismatch_rejected() {
        assert_eq!(
            Ascii::decode_request(b":1103006B00037F\r\n"),
            Err(Error::Checksum {
                expected: 0x7E,
                actual: 0x7F,
            })
        );
    }

    #[test]
    /// FR-R-112 — a character outside `0`-`9`, `A`-`F`, `a`-`f` in a
    /// hexadecimal position names the offending byte.
    fn ut_ascii_invalid_hexadecimal_character() {
        assert_eq!(
            Ascii::decode_request(b":1103006B0003GE\r\n"),
            Err(Error::InvalidCharacter(b'G'))
        );
    }

    #[test]
    /// FR-R-116 — the three ways an ADU can be misframed: no start character,
    /// no CR LF terminator, an odd number of hexadecimal characters.
    fn ut_ascii_framing_errors() {
        assert_eq!(
            Ascii::decode_request(b"1103006B00037E\r\n"),
            Err(Error::Framing {
                element: "start character",
            })
        );
        assert_eq!(
            Ascii::decode_request(b":1103006B00037E\n\r"),
            Err(Error::Framing {
                element: "terminator",
            })
        );
        assert_eq!(
            Ascii::decode_request(b":1103006B00037\r\n"),
            Err(Error::Framing {
                element: "hexadecimal character count",
            })
        );
    }

    #[test]
    /// FR-R-113 — an ASCII ADU is at most 513 characters.
    fn ut_ascii_adu_too_large() {
        let bytes = vec![b'0'; 514];
        assert_eq!(
            Ascii::decode_request(&bytes),
            Err(Error::AduTooLarge { len: 514, max: 513 })
        );
    }

    #[test]
    /// FR-R-131 — an ADU too short to hold an address, a one-byte PDU, and an
    /// LRC between its delimiters names what it expected.
    fn ut_ascii_adu_too_short() {
        assert_eq!(
            Ascii::decode_request(b":11\r\n"),
            Err(Error::Truncated {
                expected: 9,
                supplied: 5,
            })
        );
    }

    #[test]
    /// FR-R-117, FR-R-118 — ASCII carries the same address values as RTU and
    /// the same PDU; only the framing around them differs.
    fn ut_ascii_and_rtu_agree_on_address_and_pdu() {
        for address in [0x00u8, 0x01, 0xF7, 0xFF] {
            let ascii = Ascii::encode_request(&address, &read_holding()).expect("encodes");
            let rtu = Rtu::encode_request(&address, &read_holding()).expect("encodes");
            assert_eq!(
                Ascii::decode_request(&ascii),
                Rtu::decode_request(&rtu),
                "address {address:#04x}"
            );
        }
    }
}
