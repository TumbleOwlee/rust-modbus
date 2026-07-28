//! Request and response PDUs.
//!
//! Requests and responses are separate types because a PDU is not
//! self-describing: the same function code carries different layouts in each
//! direction, so the caller states the direction by choosing the type
//! (FR-R-005).

use winnow::Parser;
use winnow::binary::{be_u8, be_u16};
use winnow::token::take;

use crate::error::{Error, Result};
use crate::frame::exception::ExceptionResponse;
use crate::frame::function::FunctionCode;
use crate::parse::{self, Input, ParseResult};

/// Maximum PDU size, inclusive of the function code (FR-R-002).
pub const MAX_PDU_LEN: usize = 253;

/// Coil value meaning ON in a Write Single Coil PDU (FR-R-026).
const COIL_ON: u16 = 0xFF00;
/// Coil value meaning OFF in a Write Single Coil PDU (FR-R-026).
const COIL_OFF: u16 = 0x0000;

/// Largest quantity a Read Coils or Read Discrete Inputs request may ask for
/// (FR-R-021).
const MAX_READ_BITS: u16 = 2000;
/// Largest quantity a Read Holding or Read Input Registers request may ask for
/// (FR-R-022).
const MAX_READ_REGISTERS: u16 = 125;

/// A request PDU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestPdu {
    /// 1 — Read Coils.
    ReadCoils {
        /// Starting address.
        address: u16,
        /// Number of coils, 1–2000 (FR-R-021).
        quantity: u16,
    },
    /// 2 — Read Discrete Inputs.
    ReadDiscreteInputs {
        /// Starting address.
        address: u16,
        /// Number of inputs, 1–2000 (FR-R-021).
        quantity: u16,
    },
    /// 3 — Read Holding Registers.
    ReadHoldingRegisters {
        /// Starting address.
        address: u16,
        /// Number of registers, 1–125 (FR-R-022).
        quantity: u16,
    },
    /// 4 — Read Input Registers.
    ReadInputRegisters {
        /// Starting address.
        address: u16,
        /// Number of registers, 1–125 (FR-R-022).
        quantity: u16,
    },
    /// 5 — Write Single Coil.
    WriteSingleCoil {
        /// Output address.
        address: u16,
        /// Coil state. Encodes as `0xFF00` or `0x0000` and nothing else
        /// (FR-R-026).
        value: bool,
    },
    /// 6 — Write Single Register.
    WriteSingleRegister {
        /// Register address.
        address: u16,
        /// Value to write; any 16-bit value is permitted (FR-R-028).
        value: u16,
    },
}

/// A response PDU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsePdu {
    /// 1 — Read Coils.
    ReadCoils {
        /// Coil states, always a multiple of eight (FR-R-044).
        coils: Vec<bool>,
    },
    /// 2 — Read Discrete Inputs.
    ReadDiscreteInputs {
        /// Input states, always a multiple of eight (FR-R-044).
        inputs: Vec<bool>,
    },
    /// 3 — Read Holding Registers.
    ReadHoldingRegisters {
        /// Register values.
        registers: Vec<u16>,
    },
    /// 4 — Read Input Registers.
    ReadInputRegisters {
        /// Register values.
        registers: Vec<u16>,
    },
    /// 5 — Write Single Coil; echoes the request (FR-R-029).
    WriteSingleCoil {
        /// Output address.
        address: u16,
        /// Coil state.
        value: bool,
    },
    /// 6 — Write Single Register; echoes the request (FR-R-029).
    WriteSingleRegister {
        /// Register address.
        address: u16,
        /// Value written.
        value: u16,
    },
    /// An exception response (FR-R-081).
    Exception(ExceptionResponse),
}

/// Unpack `bytes` least significant bit first, yielding `8 × len` values
/// (FR-R-024, FR-R-044).
fn bits_from_bytes(bytes: &[u8]) -> Vec<bool> {
    bytes
        .iter()
        .flat_map(|byte| (0..8).map(move |bit| byte & (1 << bit) != 0))
        .collect()
}

/// Pack `bits` least significant bit first, zeroing the final byte's unused
/// high bits (FR-R-024).
fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .filter(|(_, set)| **set)
                .fold(0u8, |byte, (index, _)| byte | (1u8 << index))
        })
        .collect()
}

/// Check a value lies within its function code's permitted range.
fn check_range(field: &'static str, value: u16, min: u16, max: u16) -> Result<()> {
    if value < min || value > max {
        return Err(Error::OutOfRange {
            field,
            value: u32::from(value),
            min: u32::from(min),
            max: u32::from(max),
        });
    }
    Ok(())
}

/// Read a byte count and exactly that many bytes, failing if the two disagree
/// (FR-R-043). The count is checked before the data is consumed.
fn counted_bytes<'a>(input: &mut Input<'a>) -> ParseResult<&'a [u8]> {
    let expected = usize::from(be_u8.parse_next(input)?);
    let available = input.len();
    if available != expected {
        return parse::fail(Error::ByteCountMismatch {
            expected,
            actual: available,
        });
    }
    take(expected).parse_next(input)
}

/// Decode a read request body: a starting address and a quantity, the quantity
/// checked against its function code's range (FR-R-020).
fn read_request(input: &mut Input<'_>, min: u16, max: u16) -> ParseResult<(u16, u16)> {
    let address = be_u16.parse_next(input)?;
    let quantity = be_u16.parse_next(input)?;
    parse::lift(check_range("quantity", quantity, min, max))?;
    Ok((address, quantity))
}

/// Decode a single-write body, mapping the coil value through its two legal
/// encodings (FR-R-026, FR-R-027).
fn single_coil(input: &mut Input<'_>) -> ParseResult<(u16, bool)> {
    let address = be_u16.parse_next(input)?;
    let raw = be_u16.parse_next(input)?;
    let value = match raw {
        COIL_ON => true,
        COIL_OFF => false,
        other => {
            return parse::fail(Error::IllegalValue {
                field: "coil value",
                value: other,
            });
        }
    };
    Ok((address, value))
}

/// Decode a register-read response body (FR-R-025).
fn registers(input: &mut Input<'_>) -> ParseResult<Vec<u16>> {
    let data = counted_bytes(input)?;
    if data.len() % 2 != 0 {
        return parse::fail(Error::IllegalValue {
            field: "byte count",
            value: u16::try_from(data.len()).unwrap_or(u16::MAX),
        });
    }
    Ok(data
        .chunks_exact(2)
        .map(|pair| match pair {
            [hi, lo] => u16::from_be_bytes([*hi, *lo]),
            // Unreachable: `chunks_exact(2)` yields only two-element slices.
            _ => 0,
        })
        .collect())
}

/// Finish an encoded PDU, rejecting one that exceeds the maximum (FR-R-006).
fn finish(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.len() > MAX_PDU_LEN {
        return Err(Error::PduTooLarge {
            len: bytes.len(),
            max: MAX_PDU_LEN,
        });
    }
    Ok(bytes)
}

/// Encode a read request body (FR-R-020).
fn encode_read(code: u8, address: u16, quantity: u16, min: u16, max: u16) -> Result<Vec<u8>> {
    check_range("quantity", quantity, min, max)?;
    let mut bytes = vec![code];
    bytes.extend_from_slice(&address.to_be_bytes());
    bytes.extend_from_slice(&quantity.to_be_bytes());
    finish(bytes)
}

/// Encode a single-write body (FR-R-026, FR-R-028).
fn encode_single(code: u8, address: u16, value: u16) -> Result<Vec<u8>> {
    let mut bytes = vec![code];
    bytes.extend_from_slice(&address.to_be_bytes());
    bytes.extend_from_slice(&value.to_be_bytes());
    finish(bytes)
}

/// Encode a counted body: the byte count followed by the data (FR-R-023,
/// FR-R-025).
fn encode_counted(code: u8, data: &[u8]) -> Result<Vec<u8>> {
    let count = u8::try_from(data.len()).map_err(|_| Error::PduTooLarge {
        len: data.len() + 2,
        max: MAX_PDU_LEN,
    })?;
    let mut bytes = vec![code, count];
    bytes.extend_from_slice(data);
    finish(bytes)
}

impl RequestPdu {
    /// Decode a request PDU.
    ///
    /// Quantity ranges are checked here as well as on encode: FR-R-133 requires
    /// that whatever decodes re-encodes identically, so a PDU the encoder would
    /// reject must not decode either.
    pub fn decode(pdu: &[u8]) -> Result<Self> {
        parse::run(pdu, |input: &mut Input<'_>| {
            let code = parse::lift(FunctionCode::decode(be_u8.parse_next(input)?))?;
            let request = match code {
                FunctionCode::ReadCoils => {
                    let (address, quantity) = read_request(input, 1, MAX_READ_BITS)?;
                    Self::ReadCoils { address, quantity }
                }
                FunctionCode::ReadDiscreteInputs => {
                    let (address, quantity) = read_request(input, 1, MAX_READ_BITS)?;
                    Self::ReadDiscreteInputs { address, quantity }
                }
                FunctionCode::ReadHoldingRegisters => {
                    let (address, quantity) = read_request(input, 1, MAX_READ_REGISTERS)?;
                    Self::ReadHoldingRegisters { address, quantity }
                }
                FunctionCode::ReadInputRegisters => {
                    let (address, quantity) = read_request(input, 1, MAX_READ_REGISTERS)?;
                    Self::ReadInputRegisters { address, quantity }
                }
                FunctionCode::WriteSingleCoil => {
                    let (address, value) = single_coil(input)?;
                    Self::WriteSingleCoil { address, value }
                }
                FunctionCode::WriteSingleRegister => {
                    let address = be_u16.parse_next(input)?;
                    let value = be_u16.parse_next(input)?;
                    Self::WriteSingleRegister { address, value }
                }
                // Bodies for the remaining function codes land in later stages;
                // this arm shrinks to nothing when the last of them does.
                _ => return parse::fail(Error::Malformed),
            };
            Ok(request)
        })
    }

    /// Encode to a request PDU.
    pub fn encode(&self) -> Result<Vec<u8>> {
        match *self {
            Self::ReadCoils { address, quantity } => {
                encode_read(1, address, quantity, 1, MAX_READ_BITS)
            }
            Self::ReadDiscreteInputs { address, quantity } => {
                encode_read(2, address, quantity, 1, MAX_READ_BITS)
            }
            Self::ReadHoldingRegisters { address, quantity } => {
                encode_read(3, address, quantity, 1, MAX_READ_REGISTERS)
            }
            Self::ReadInputRegisters { address, quantity } => {
                encode_read(4, address, quantity, 1, MAX_READ_REGISTERS)
            }
            Self::WriteSingleCoil { address, value } => {
                encode_single(5, address, if value { COIL_ON } else { COIL_OFF })
            }
            Self::WriteSingleRegister { address, value } => encode_single(6, address, value),
        }
    }
}

impl ResponsePdu {
    /// Decode a response PDU.
    pub fn decode(pdu: &[u8]) -> Result<Self> {
        if pdu.first().is_some_and(|byte| byte & 0x80 != 0) {
            return ExceptionResponse::decode(pdu).map(Self::Exception);
        }
        parse::run(pdu, |input: &mut Input<'_>| {
            let code = parse::lift(FunctionCode::decode(be_u8.parse_next(input)?))?;
            let response = match code {
                FunctionCode::ReadCoils => Self::ReadCoils {
                    coils: bits_from_bytes(counted_bytes(input)?),
                },
                FunctionCode::ReadDiscreteInputs => Self::ReadDiscreteInputs {
                    inputs: bits_from_bytes(counted_bytes(input)?),
                },
                FunctionCode::ReadHoldingRegisters => Self::ReadHoldingRegisters {
                    registers: registers(input)?,
                },
                FunctionCode::ReadInputRegisters => Self::ReadInputRegisters {
                    registers: registers(input)?,
                },
                FunctionCode::WriteSingleCoil => {
                    let (address, value) = single_coil(input)?;
                    Self::WriteSingleCoil { address, value }
                }
                FunctionCode::WriteSingleRegister => {
                    let address = be_u16.parse_next(input)?;
                    let value = be_u16.parse_next(input)?;
                    Self::WriteSingleRegister { address, value }
                }
                // As in `RequestPdu::decode`: later stages fill this in.
                _ => return parse::fail(Error::Malformed),
            };
            Ok(response)
        })
    }

    /// Encode to a response PDU.
    pub fn encode(&self) -> Result<Vec<u8>> {
        match self {
            Self::ReadCoils { coils } => encode_counted(1, &bits_to_bytes(coils)),
            Self::ReadDiscreteInputs { inputs } => encode_counted(2, &bits_to_bytes(inputs)),
            Self::ReadHoldingRegisters { registers } => {
                encode_counted(3, &registers_to_bytes(registers))
            }
            Self::ReadInputRegisters { registers } => {
                encode_counted(4, &registers_to_bytes(registers))
            }
            Self::WriteSingleCoil { address, value } => {
                encode_single(5, *address, if *value { COIL_ON } else { COIL_OFF })
            }
            Self::WriteSingleRegister { address, value } => encode_single(6, *address, *value),
            Self::Exception(exception) => exception.encode(),
        }
    }
}

/// Flatten registers to big-endian bytes (FR-R-003).
fn registers_to_bytes(registers: &[u16]) -> Vec<u8> {
    registers
        .iter()
        .flat_map(|register| register.to_be_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::exception::ExceptionCode;

    /// Coil states of the Read Coils response in the specification's worked
    /// example: bytes `CD 6B 05`, least significant bit first (§6.1).
    fn spec_example_coils() -> Vec<bool> {
        [0xCDu8, 0x6B, 0x05]
            .iter()
            .flat_map(|byte| (0..8).map(move |bit| byte & (1 << bit) != 0))
            .collect()
    }

    #[test]
    /// FR-R-020 — a read request is a 2-byte starting address followed by a
    /// 2-byte quantity. Bytes from the specification's Read Coils example
    /// (§6.1): address 19, quantity 19.
    fn ut_read_coils_request_bytes() {
        let request = RequestPdu::ReadCoils {
            address: 19,
            quantity: 19,
        };
        let bytes = vec![0x01, 0x00, 0x13, 0x00, 0x13];
        assert_eq!(request.encode(), Ok(bytes.clone()));
        assert_eq!(RequestPdu::decode(&bytes), Ok(request));
    }

    #[test]
    /// FR-R-023 — a bit-read response is a byte count followed by that many data
    /// bytes, the count being `(quantity + 7) / 8`.
    /// FR-R-024 — bit *n* sits in bit `n mod 8` of data byte `n / 8`, least
    /// significant bit first. Bytes from the specification's example (§6.1).
    fn ut_read_coils_response_bit_packing() {
        let bytes = vec![0x01, 0x03, 0xCD, 0x6B, 0x05];
        let response = ResponsePdu::ReadCoils {
            coils: spec_example_coils(),
        };
        assert_eq!(ResponsePdu::decode(&bytes), Ok(response.clone()));
        assert_eq!(response.encode(), Ok(bytes));

        // The first coil of the example is ON and the second OFF (0xCD = 1100_1101).
        let coils = spec_example_coils();
        assert_eq!(coils.first(), Some(&true));
        assert_eq!(coils.get(1), Some(&false));
    }

    #[test]
    /// FR-R-044 — a bit-read response carries no coil count, so decoding yields
    /// exactly `8 × byte count` values including the final byte's padding.
    fn ut_bit_response_yields_byte_count_times_eight() {
        let decoded = ResponsePdu::decode(&[0x01, 0x03, 0xCD, 0x6B, 0x05]);
        let ResponsePdu::ReadCoils { coils } = decoded.expect("valid response") else {
            panic!("expected a Read Coils response");
        };
        assert_eq!(coils.len(), 24);
    }

    #[test]
    /// FR-R-020 — Read Discrete Inputs shares the read request layout. Bytes
    /// from the specification's example (§6.2): address 196, quantity 22.
    fn ut_read_discrete_inputs_spec_example() {
        let request = RequestPdu::ReadDiscreteInputs {
            address: 196,
            quantity: 22,
        };
        let request_bytes = vec![0x02, 0x00, 0xC4, 0x00, 0x16];
        assert_eq!(request.encode(), Ok(request_bytes.clone()));
        assert_eq!(RequestPdu::decode(&request_bytes), Ok(request));

        let response_bytes = vec![0x02, 0x03, 0xAC, 0xDB, 0x35];
        let decoded = ResponsePdu::decode(&response_bytes).expect("valid response");
        assert_eq!(decoded.encode(), Ok(response_bytes));
    }

    #[test]
    /// FR-R-025 — a register-read response is a byte count of `2 × quantity`
    /// followed by that many data bytes. Bytes from the specification's Read
    /// Holding Registers example (§6.3): address 107, quantity 3.
    fn ut_read_holding_registers_spec_example() {
        let request = RequestPdu::ReadHoldingRegisters {
            address: 107,
            quantity: 3,
        };
        let request_bytes = vec![0x03, 0x00, 0x6B, 0x00, 0x03];
        assert_eq!(request.encode(), Ok(request_bytes.clone()));
        assert_eq!(RequestPdu::decode(&request_bytes), Ok(request));

        let response = ResponsePdu::ReadHoldingRegisters {
            registers: vec![0x022B, 0x0000, 0x0064],
        };
        let response_bytes = vec![0x03, 0x06, 0x02, 0x2B, 0x00, 0x00, 0x00, 0x64];
        assert_eq!(response.encode(), Ok(response_bytes.clone()));
        assert_eq!(ResponsePdu::decode(&response_bytes), Ok(response));
    }

    #[test]
    /// FR-R-025 — Read Input Registers shares the register-read response layout.
    /// Bytes from the specification's example (§6.4): address 8, quantity 1.
    fn ut_read_input_registers_spec_example() {
        let request = RequestPdu::ReadInputRegisters {
            address: 8,
            quantity: 1,
        };
        let request_bytes = vec![0x04, 0x00, 0x08, 0x00, 0x01];
        assert_eq!(request.encode(), Ok(request_bytes.clone()));
        assert_eq!(RequestPdu::decode(&request_bytes), Ok(request));

        let response = ResponsePdu::ReadInputRegisters {
            registers: vec![0x000A],
        };
        let response_bytes = vec![0x04, 0x02, 0x00, 0x0A];
        assert_eq!(response.encode(), Ok(response_bytes.clone()));
        assert_eq!(ResponsePdu::decode(&response_bytes), Ok(response));
    }

    #[test]
    /// FR-R-026 — Write Single Coil carries `0xFF00` for ON and `0x0000` for
    /// OFF. Bytes from the specification's example (§6.5): address 172, ON.
    /// FR-R-029 — the response echoes the request byte for byte.
    fn ut_write_single_coil_spec_example() {
        let request = RequestPdu::WriteSingleCoil {
            address: 172,
            value: true,
        };
        let bytes = vec![0x05, 0x00, 0xAC, 0xFF, 0x00];
        assert_eq!(request.encode(), Ok(bytes.clone()));
        assert_eq!(RequestPdu::decode(&bytes), Ok(request));

        let response = ResponsePdu::WriteSingleCoil {
            address: 172,
            value: true,
        };
        assert_eq!(response.encode(), Ok(bytes.clone()));
        assert_eq!(ResponsePdu::decode(&bytes), Ok(response));
    }

    #[test]
    /// FR-R-026 — an OFF coil encodes as `0x0000`.
    fn ut_write_single_coil_off() {
        let request = RequestPdu::WriteSingleCoil {
            address: 172,
            value: false,
        };
        let bytes = vec![0x05, 0x00, 0xAC, 0x00, 0x00];
        assert_eq!(request.encode(), Ok(bytes.clone()));
        assert_eq!(RequestPdu::decode(&bytes), Ok(request));
    }

    #[test]
    /// FR-R-027 — a coil value that is neither `0xFF00` nor `0x0000` is an
    /// illegal-value error in both directions.
    fn ut_write_single_coil_value_illegal() {
        for value in [0x0001u16, 0x00FF, 0xFF01, 0xFFFF] {
            let [hi, lo] = value.to_be_bytes();
            let expected = Error::IllegalValue {
                field: "coil value",
                value,
            };
            assert_eq!(
                RequestPdu::decode(&[0x05, 0x00, 0xAC, hi, lo]),
                Err(expected.clone()),
                "request value {value:#06x}"
            );
            assert_eq!(
                ResponsePdu::decode(&[0x05, 0x00, 0xAC, hi, lo]),
                Err(expected),
                "response value {value:#06x}"
            );
        }
    }

    #[test]
    /// FR-R-028 — Write Single Register accepts any 16-bit value. Bytes from the
    /// specification's example (§6.6): address 1, value 3.
    fn ut_write_single_register_spec_example() {
        let request = RequestPdu::WriteSingleRegister {
            address: 1,
            value: 3,
        };
        let bytes = vec![0x06, 0x00, 0x01, 0x00, 0x03];
        assert_eq!(request.encode(), Ok(bytes.clone()));
        assert_eq!(RequestPdu::decode(&bytes), Ok(request));

        for value in [0x0000u16, 0x00FF, 0xFF00, 0xFFFF] {
            let [hi, lo] = value.to_be_bytes();
            assert_eq!(
                RequestPdu::decode(&[0x06, 0x00, 0x01, hi, lo]),
                Ok(RequestPdu::WriteSingleRegister { address: 1, value }),
                "value {value:#06x}"
            );
        }
    }

    #[test]
    /// FR-R-021 — a coil quantity outside 1–2000 is an out-of-range error.
    /// FR-R-045 — and it is rejected on decode as well as on encode.
    fn ut_coil_quantity_out_of_range() {
        for quantity in [0u16, 2001, 65535] {
            let expected = Error::OutOfRange {
                field: "quantity",
                value: u32::from(quantity),
                min: 1,
                max: 2000,
            };
            assert_eq!(
                RequestPdu::ReadCoils {
                    address: 0,
                    quantity
                }
                .encode(),
                Err(expected.clone()),
                "quantity {quantity}"
            );
            let [hi, lo] = quantity.to_be_bytes();
            assert_eq!(
                RequestPdu::decode(&[0x01, 0x00, 0x00, hi, lo]),
                Err(expected),
                "decoding quantity {quantity}"
            );
        }
    }

    #[test]
    /// FR-R-022 — a register quantity outside 1–125 is an out-of-range error.
    /// FR-R-045 — and it is rejected on decode as well as on encode.
    fn ut_register_quantity_out_of_range() {
        for quantity in [0u16, 126, 2000] {
            let expected = Error::OutOfRange {
                field: "quantity",
                value: u32::from(quantity),
                min: 1,
                max: 125,
            };
            assert_eq!(
                RequestPdu::ReadHoldingRegisters {
                    address: 0,
                    quantity
                }
                .encode(),
                Err(expected.clone()),
                "quantity {quantity}"
            );
            let [hi, lo] = quantity.to_be_bytes();
            assert_eq!(
                RequestPdu::decode(&[0x03, 0x00, 0x00, hi, lo]),
                Err(expected),
                "decoding quantity {quantity}"
            );
        }
    }

    #[test]
    /// FR-R-043 — a byte count disagreeing with the data present is a
    /// byte-count-mismatch error, raised before any data byte is consumed.
    fn ut_byte_count_mismatch() {
        assert_eq!(
            ResponsePdu::decode(&[0x03, 0x06, 0x02, 0x2B, 0x00, 0x00]),
            Err(Error::ByteCountMismatch {
                expected: 6,
                actual: 4,
            })
        );
        assert_eq!(
            ResponsePdu::decode(&[0x01, 0x03, 0xCD, 0x6B]),
            Err(Error::ByteCountMismatch {
                expected: 3,
                actual: 2,
            })
        );
    }

    #[test]
    /// FR-R-046 — a register-read response byte count must be even, since no
    /// quantity of 16-bit registers can produce an odd one.
    fn ut_register_response_byte_count_must_be_even() {
        assert_eq!(
            ResponsePdu::decode(&[0x03, 0x03, 0x02, 0x2B, 0x00]),
            Err(Error::IllegalValue {
                field: "byte count",
                value: 3,
            })
        );
    }

    #[test]
    /// FR-R-002 — a PDU is at most 253 bytes.
    /// FR-R-006 — encoding one that would exceed it fails with a size error
    /// rather than emitting an oversized PDU.
    fn ut_pdu_exceeding_max_is_rejected() {
        // 126 registers would need 1 + 1 + 252 = 254 bytes.
        let response = ResponsePdu::ReadHoldingRegisters {
            registers: vec![0; 126],
        };
        assert_eq!(
            response.encode(),
            Err(Error::PduTooLarge {
                len: 254,
                max: MAX_PDU_LEN,
            })
        );
    }

    #[test]
    /// FR-R-002 — the largest legal register-read response encodes at exactly
    /// the 253-byte maximum.
    fn ut_max_size_pdu_is_accepted() {
        let response = ResponsePdu::ReadHoldingRegisters {
            registers: vec![0; 125],
        };
        let bytes = response.encode().expect("125 registers fit");
        assert_eq!(bytes.len(), 252);
    }

    #[test]
    /// FR-R-131 — a request shorter than its layout requires reports the bytes
    /// expected and supplied.
    fn ut_truncated_request_is_reported() {
        assert_eq!(
            RequestPdu::decode(&[0x01, 0x00, 0x13, 0x00]),
            Err(Error::Truncated {
                expected: 5,
                supplied: 4,
            })
        );
    }

    #[test]
    /// FR-R-132 — surplus bytes after a complete PDU are rejected.
    fn ut_trailing_bytes_are_rejected() {
        assert_eq!(
            RequestPdu::decode(&[0x01, 0x00, 0x13, 0x00, 0x13, 0xFF]),
            Err(Error::TrailingBytes { extra: 1 })
        );
    }

    #[test]
    /// FR-R-081 — a response whose function code has the high bit set decodes as
    /// an exception response.
    /// FR-R-086 — including for a custom function code.
    fn ut_response_decodes_exception() {
        assert_eq!(
            ResponsePdu::decode(&[0x83, 0x02]),
            Ok(ResponsePdu::Exception(ExceptionResponse {
                function: FunctionCode::ReadHoldingRegisters,
                exception: ExceptionCode::IllegalDataAddress,
            }))
        );
        assert_eq!(
            ResponsePdu::decode(&[0x80 | 100, 0x0B]),
            Ok(ResponsePdu::Exception(ExceptionResponse {
                function: FunctionCode::Custom(100),
                exception: ExceptionCode::GatewayTargetDeviceFailedToRespond,
            }))
        );
    }

    #[test]
    /// FR-R-133 — decode and encode are inverse for every PDU in this stage.
    fn ut_round_trips() {
        let requests: Vec<Vec<u8>> = vec![
            vec![0x01, 0x00, 0x13, 0x00, 0x13],
            vec![0x02, 0x00, 0xC4, 0x00, 0x16],
            vec![0x03, 0x00, 0x6B, 0x00, 0x03],
            vec![0x04, 0x00, 0x08, 0x00, 0x01],
            vec![0x05, 0x00, 0xAC, 0xFF, 0x00],
            vec![0x05, 0x00, 0xAC, 0x00, 0x00],
            vec![0x06, 0x00, 0x01, 0x00, 0x03],
        ];
        for bytes in requests {
            let decoded = RequestPdu::decode(&bytes).expect("valid request");
            assert_eq!(decoded.encode(), Ok(bytes.clone()), "request {bytes:?}");
        }

        let responses: Vec<Vec<u8>> = vec![
            vec![0x01, 0x03, 0xCD, 0x6B, 0x05],
            vec![0x02, 0x03, 0xAC, 0xDB, 0x35],
            vec![0x03, 0x06, 0x02, 0x2B, 0x00, 0x00, 0x00, 0x64],
            vec![0x04, 0x02, 0x00, 0x0A],
            vec![0x05, 0x00, 0xAC, 0xFF, 0x00],
            vec![0x06, 0x00, 0x01, 0x00, 0x03],
            vec![0x83, 0x02],
        ];
        for bytes in responses {
            let decoded = ResponsePdu::decode(&bytes).expect("valid response");
            assert_eq!(decoded.encode(), Ok(bytes.clone()), "response {bytes:?}");
        }
    }
}
