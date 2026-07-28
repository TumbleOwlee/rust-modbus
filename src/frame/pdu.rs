//! Request and response PDUs.
//!
//! Requests and responses are separate types because a PDU is not
//! self-describing: the same function code carries different layouts in each
//! direction, so the caller states the direction by choosing the type
//! (FR-R-005).

use winnow::Parser;
use winnow::binary::{be_u8, be_u16};
use winnow::token::{rest, take};

use crate::error::{Error, Result};
use crate::frame::diagnostics::DiagnosticSubFunction;
use crate::frame::exception::ExceptionResponse;
use crate::frame::file::{self, FileRecordRead, FileRecordReadResponse, FileRecordWrite};
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
    /// 15 — Write Multiple Coils.
    WriteMultipleCoils {
        /// Starting address.
        address: u16,
        /// Coil states; 1–1968 of them (FR-R-031).
        coils: Vec<bool>,
    },
    /// 16 — Write Multiple Registers.
    WriteMultipleRegisters {
        /// Starting address.
        address: u16,
        /// Values to write; 1–123 of them (FR-R-033).
        registers: Vec<u16>,
    },
    /// 22 — Mask Write Register.
    MaskWriteRegister {
        /// Reference address.
        address: u16,
        /// AND mask.
        and_mask: u16,
        /// OR mask.
        or_mask: u16,
    },
    /// 23 — Read/Write Multiple Registers. The write is performed before the
    /// read (FR-R-037).
    ReadWriteMultipleRegisters {
        /// Starting address of the read.
        read_address: u16,
        /// Number of registers to read; 1–125 (FR-R-038).
        read_quantity: u16,
        /// Starting address of the write.
        write_address: u16,
        /// Values to write; 1–121 of them (FR-R-038).
        registers: Vec<u16>,
    },
    /// 7 — Read Exception Status (FR-R-060).
    ReadExceptionStatus,
    /// 8 — Diagnostics.
    Diagnostics {
        /// Sub-function to run (FR-R-062).
        sub_function: DiagnosticSubFunction,
        /// Sub-function data words; may be empty (FR-R-061).
        data: Vec<u16>,
    },
    /// 11 — Get Comm Event Counter (FR-R-064).
    GetCommEventCounter,
    /// 12 — Get Comm Event Log (FR-R-065).
    GetCommEventLog,
    /// 17 — Report Server ID (FR-R-066).
    ReportServerId,
    /// 20 — Read File Record.
    ReadFileRecord {
        /// Sub-requests, 7 bytes each; 1–35 of them (FR-R-050, FR-R-051).
        records: Vec<FileRecordRead>,
    },
    /// 21 — Write File Record.
    WriteFileRecord {
        /// Sub-requests (FR-R-053).
        records: Vec<FileRecordWrite>,
    },
    /// 24 — Read FIFO Queue.
    ReadFifoQueue {
        /// FIFO pointer address.
        address: u16,
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
    /// 15 — Write Multiple Coils; echoes address and quantity (FR-R-034).
    WriteMultipleCoils {
        /// Starting address.
        address: u16,
        /// Number of coils written.
        quantity: u16,
    },
    /// 16 — Write Multiple Registers; echoes address and quantity (FR-R-034).
    WriteMultipleRegisters {
        /// Starting address.
        address: u16,
        /// Number of registers written.
        quantity: u16,
    },
    /// 22 — Mask Write Register; echoes the request (FR-R-035).
    MaskWriteRegister {
        /// Reference address.
        address: u16,
        /// AND mask.
        and_mask: u16,
        /// OR mask.
        or_mask: u16,
    },
    /// 23 — Read/Write Multiple Registers.
    ReadWriteMultipleRegisters {
        /// Values read.
        registers: Vec<u16>,
    },
    /// 7 — Read Exception Status.
    ReadExceptionStatus {
        /// The eight exception status outputs (FR-R-060).
        status: u8,
    },
    /// 8 — Diagnostics.
    Diagnostics {
        /// Sub-function echoed back (FR-R-061).
        sub_function: DiagnosticSubFunction,
        /// Response data words; may be empty (FR-R-061).
        data: Vec<u16>,
    },
    /// 11 — Get Comm Event Counter.
    GetCommEventCounter {
        /// `0xFFFF` while busy, `0x0000` otherwise; carried as-is (FR-R-068).
        status: u16,
        /// Event counter (FR-R-064).
        event_count: u16,
    },
    /// 12 — Get Comm Event Log.
    GetCommEventLog {
        /// `0xFFFF` while busy, `0x0000` otherwise; carried as-is (FR-R-068).
        status: u16,
        /// Event counter (FR-R-065).
        event_count: u16,
        /// Message counter (FR-R-065).
        message_count: u16,
        /// Event bytes, most recent first; at most 64 (FR-R-065).
        events: Vec<u8>,
    },
    /// 17 — Report Server ID.
    ReportServerId {
        /// The device-specific body, carried whole (FR-R-066).
        data: Vec<u8>,
    },
    /// 20 — Read File Record.
    ReadFileRecord {
        /// Sub-responses, one per sub-request (FR-R-052).
        records: Vec<FileRecordReadResponse>,
    },
    /// 21 — Write File Record — an echo of the request (FR-R-053).
    WriteFileRecord {
        /// Sub-requests echoed back.
        records: Vec<FileRecordWrite>,
    },
    /// 24 — Read FIFO Queue.
    ReadFifoQueue {
        /// Queued values; at most 31 (FR-R-042).
        values: Vec<u16>,
    },
    /// An exception response (FR-R-081).
    Exception(ExceptionResponse),
}

/// The value a Mask Write Register request produces from the register's current
/// contents (FR-R-036).
///
/// Applying it to stored data is the server's behavior; defining it is the frame
/// layer's.
#[must_use]
pub fn mask_write_result(current: u16, and_mask: u16, or_mask: u16) -> u16 {
    (current & and_mask) | (or_mask & !and_mask)
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

/// Largest quantity a Write Multiple Coils request may carry (FR-R-031).
const MAX_WRITE_BITS: u16 = 1968;
/// Largest quantity a Write Multiple Registers request may carry (FR-R-033).
const MAX_WRITE_REGISTERS: u16 = 123;
/// Largest write quantity a Read/Write Multiple Registers request may carry
/// (FR-R-038).
const MAX_RW_WRITE_REGISTERS: u16 = 121;
/// Largest FIFO count a Read FIFO Queue response may report (FR-R-042).
const MAX_FIFO_COUNT: u16 = 31;

/// Decode a bit-packed body of exactly `quantity` bits, rejecting non-zero
/// padding above it (FR-R-024, FR-R-047).
fn packed_bits(data: &[u8], quantity: usize) -> Result<Vec<bool>> {
    let mut bits = bits_from_bytes(data);
    if let Some(padding) = bits.get(quantity..)
        && padding.iter().any(|set| *set)
    {
        let last = data.last().copied().unwrap_or_default();
        return Err(Error::IllegalValue {
            field: "coil padding",
            value: u16::from(last),
        });
    }
    bits.truncate(quantity);
    Ok(bits)
}

/// Read a quantity, a byte count derived from it, and that many data bytes
/// (FR-R-030, FR-R-032, FR-R-037, FR-R-043).
fn quantified_bytes<'a>(
    input: &mut Input<'a>,
    field: &'static str,
    min: u16,
    max: u16,
    width: fn(usize) -> usize,
) -> ParseResult<(u16, &'a [u8])> {
    let quantity = be_u16.parse_next(input)?;
    parse::lift(check_range(field, quantity, min, max))?;
    let expected = width(usize::from(quantity));
    let count = usize::from(be_u8.parse_next(input)?);
    if count != expected {
        return parse::fail(Error::ByteCountMismatch {
            expected,
            actual: count,
        });
    }
    let available = input.len();
    if available < expected {
        return parse::fail(Error::ByteCountMismatch {
            expected,
            actual: available,
        });
    }
    let data = take(expected).parse_next(input)?;
    Ok((quantity, data))
}

/// Bytes occupied by `quantity` bits (FR-R-030).
fn bit_width(quantity: usize) -> usize {
    quantity.div_ceil(8)
}

/// Bytes occupied by `quantity` registers (FR-R-032).
fn register_width(quantity: usize) -> usize {
    quantity * 2
}

/// Interpret `data` as big-endian registers (FR-R-003).
pub(super) fn registers_from_bytes(data: &[u8]) -> Vec<u16> {
    data.chunks_exact(2)
        .map(|pair| match pair {
            [hi, lo] => u16::from_be_bytes([*hi, *lo]),
            // Unreachable: `chunks_exact(2)` yields only two-element slices.
            _ => 0,
        })
        .collect()
}

/// Quantity of a collection, checked against its function code's range
/// (FR-R-031, FR-R-033, FR-R-038).
fn quantity_of(field: &'static str, len: usize, min: u16, max: u16) -> Result<u16> {
    let quantity = u16::try_from(len).unwrap_or(u16::MAX);
    check_range(field, quantity, min, max)?;
    Ok(quantity)
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

/// Encode an address, a quantity, a byte count and the data (FR-R-030,
/// FR-R-032).
fn encode_quantified(code: u8, address: u16, quantity: u16, data: &[u8]) -> Result<Vec<u8>> {
    let count = u8::try_from(data.len()).map_err(|_| Error::PduTooLarge {
        len: data.len() + 6,
        max: MAX_PDU_LEN,
    })?;
    let mut bytes = vec![code];
    bytes.extend_from_slice(&address.to_be_bytes());
    bytes.extend_from_slice(&quantity.to_be_bytes());
    bytes.push(count);
    bytes.extend_from_slice(data);
    finish(bytes)
}

/// Encode an address and a quantity, the shape of a multiple-write response
/// (FR-R-034).
fn encode_echo_quantity(code: u8, address: u16, quantity: u16) -> Result<Vec<u8>> {
    let mut bytes = vec![code];
    bytes.extend_from_slice(&address.to_be_bytes());
    bytes.extend_from_slice(&quantity.to_be_bytes());
    finish(bytes)
}

/// Fixed bytes a Get Comm Event Log byte count always covers: the status,
/// event count, and message count words (FR-R-065).
const EVENT_LOG_FIXED_BYTES: u32 = 6;

/// Most event bytes a Get Comm Event Log response may carry (FR-R-065).
const MAX_EVENT_BYTES: u32 = 64;

/// A Diagnostics body: the sub-function code, then whole 16-bit data words
/// (FR-R-061, FR-R-062).
fn diagnostic_body(input: &mut Input<'_>) -> ParseResult<(DiagnosticSubFunction, Vec<u16>)> {
    let sub_function = DiagnosticSubFunction::decode(be_u16.parse_next(input)?);
    let data = rest.parse_next(input)?;
    if data.len() % 2 != 0 {
        return parse::fail(Error::IllegalValue {
            field: "diagnostic data length",
            value: u16::try_from(data.len()).unwrap_or(u16::MAX),
        });
    }
    Ok((sub_function, registers_from_bytes(data)))
}

/// Encode a Diagnostics body under `code` (FR-R-061).
fn encode_diagnostics(
    code: u8,
    sub_function: DiagnosticSubFunction,
    data: &[u16],
) -> Result<Vec<u8>> {
    let mut bytes = vec![code];
    bytes.extend_from_slice(&sub_function.encode()?.to_be_bytes());
    bytes.extend_from_slice(&registers_to_bytes(data));
    finish(bytes)
}

/// Check a Get Comm Event Log byte count against the range its layout fixes
/// (FR-R-065).
fn check_event_log_bytes(byte_count: u32) -> Result<()> {
    let max = EVENT_LOG_FIXED_BYTES.saturating_add(MAX_EVENT_BYTES);
    if byte_count < EVENT_LOG_FIXED_BYTES || byte_count > max {
        return Err(Error::OutOfRange {
            field: "byte count",
            value: byte_count,
            min: EVENT_LOG_FIXED_BYTES,
            max,
        });
    }
    Ok(())
}

/// Prepend `code` to an already-assembled body and size-check the result.
fn encode_body(code: u8, body: Vec<u8>) -> Result<Vec<u8>> {
    let mut bytes = vec![code];
    bytes.extend_from_slice(&body);
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
                FunctionCode::WriteMultipleCoils => {
                    let address = be_u16.parse_next(input)?;
                    let (quantity, data) =
                        quantified_bytes(input, "quantity", 1, MAX_WRITE_BITS, bit_width)?;
                    Self::WriteMultipleCoils {
                        address,
                        coils: parse::lift(packed_bits(data, usize::from(quantity)))?,
                    }
                }
                FunctionCode::WriteMultipleRegisters => {
                    let address = be_u16.parse_next(input)?;
                    let (_, data) = quantified_bytes(
                        input,
                        "quantity",
                        1,
                        MAX_WRITE_REGISTERS,
                        register_width,
                    )?;
                    Self::WriteMultipleRegisters {
                        address,
                        registers: registers_from_bytes(data),
                    }
                }
                FunctionCode::MaskWriteRegister => Self::MaskWriteRegister {
                    address: be_u16.parse_next(input)?,
                    and_mask: be_u16.parse_next(input)?,
                    or_mask: be_u16.parse_next(input)?,
                },
                FunctionCode::ReadWriteMultipleRegisters => {
                    let read_address = be_u16.parse_next(input)?;
                    let read_quantity = be_u16.parse_next(input)?;
                    parse::lift(check_range(
                        "read quantity",
                        read_quantity,
                        1,
                        MAX_READ_REGISTERS,
                    ))?;
                    let write_address = be_u16.parse_next(input)?;
                    let (_, data) = quantified_bytes(
                        input,
                        "write quantity",
                        1,
                        MAX_RW_WRITE_REGISTERS,
                        register_width,
                    )?;
                    Self::ReadWriteMultipleRegisters {
                        read_address,
                        read_quantity,
                        write_address,
                        registers: registers_from_bytes(data),
                    }
                }
                FunctionCode::ReadExceptionStatus => Self::ReadExceptionStatus,
                FunctionCode::Diagnostics => {
                    let (sub_function, data) = diagnostic_body(input)?;
                    Self::Diagnostics { sub_function, data }
                }
                FunctionCode::GetCommEventCounter => Self::GetCommEventCounter,
                FunctionCode::GetCommEventLog => Self::GetCommEventLog,
                FunctionCode::ReportServerId => Self::ReportServerId,
                FunctionCode::ReadFileRecord => Self::ReadFileRecord {
                    records: file::decode_read_requests(input)?,
                },
                FunctionCode::WriteFileRecord => Self::WriteFileRecord {
                    records: file::decode_write_records(input)?,
                },
                FunctionCode::ReadFifoQueue => Self::ReadFifoQueue {
                    address: be_u16.parse_next(input)?,
                },
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
            Self::WriteMultipleCoils { address, ref coils } => {
                let quantity = quantity_of("quantity", coils.len(), 1, MAX_WRITE_BITS)?;
                encode_quantified(15, address, quantity, &bits_to_bytes(coils))
            }
            Self::WriteMultipleRegisters {
                address,
                ref registers,
            } => {
                let quantity = quantity_of("quantity", registers.len(), 1, MAX_WRITE_REGISTERS)?;
                encode_quantified(16, address, quantity, &registers_to_bytes(registers))
            }
            Self::MaskWriteRegister {
                address,
                and_mask,
                or_mask,
            } => {
                let mut bytes = vec![22];
                bytes.extend_from_slice(&address.to_be_bytes());
                bytes.extend_from_slice(&and_mask.to_be_bytes());
                bytes.extend_from_slice(&or_mask.to_be_bytes());
                finish(bytes)
            }
            Self::ReadWriteMultipleRegisters {
                read_address,
                read_quantity,
                write_address,
                ref registers,
            } => {
                check_range("read quantity", read_quantity, 1, MAX_READ_REGISTERS)?;
                let write_quantity =
                    quantity_of("write quantity", registers.len(), 1, MAX_RW_WRITE_REGISTERS)?;
                let mut bytes = vec![23];
                bytes.extend_from_slice(&read_address.to_be_bytes());
                bytes.extend_from_slice(&read_quantity.to_be_bytes());
                bytes.extend_from_slice(&write_address.to_be_bytes());
                bytes.extend_from_slice(&write_quantity.to_be_bytes());
                let data = registers_to_bytes(registers);
                bytes.push(u8::try_from(data.len()).unwrap_or(u8::MAX));
                bytes.extend_from_slice(&data);
                finish(bytes)
            }
            Self::ReadExceptionStatus => finish(vec![7]),
            Self::Diagnostics {
                sub_function,
                ref data,
            } => encode_diagnostics(8, sub_function, data),
            Self::GetCommEventCounter => finish(vec![11]),
            Self::GetCommEventLog => finish(vec![12]),
            Self::ReportServerId => finish(vec![17]),
            Self::ReadFileRecord { ref records } => {
                encode_body(20, file::encode_read_requests(records)?)
            }
            Self::WriteFileRecord { ref records } => {
                encode_body(21, file::encode_write_records(records)?)
            }
            Self::ReadFifoQueue { address } => {
                let mut bytes = vec![24];
                bytes.extend_from_slice(&address.to_be_bytes());
                finish(bytes)
            }
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
                FunctionCode::WriteMultipleCoils => Self::WriteMultipleCoils {
                    address: be_u16.parse_next(input)?,
                    quantity: be_u16.parse_next(input)?,
                },
                FunctionCode::WriteMultipleRegisters => Self::WriteMultipleRegisters {
                    address: be_u16.parse_next(input)?,
                    quantity: be_u16.parse_next(input)?,
                },
                FunctionCode::MaskWriteRegister => Self::MaskWriteRegister {
                    address: be_u16.parse_next(input)?,
                    and_mask: be_u16.parse_next(input)?,
                    or_mask: be_u16.parse_next(input)?,
                },
                FunctionCode::ReadWriteMultipleRegisters => Self::ReadWriteMultipleRegisters {
                    registers: registers(input)?,
                },
                FunctionCode::ReadExceptionStatus => Self::ReadExceptionStatus {
                    status: be_u8.parse_next(input)?,
                },
                FunctionCode::Diagnostics => {
                    let (sub_function, data) = diagnostic_body(input)?;
                    Self::Diagnostics { sub_function, data }
                }
                FunctionCode::GetCommEventCounter => Self::GetCommEventCounter {
                    status: be_u16.parse_next(input)?,
                    event_count: be_u16.parse_next(input)?,
                },
                FunctionCode::GetCommEventLog => {
                    let byte_count = u32::from(be_u8.parse_next(input)?);
                    parse::lift(check_event_log_bytes(byte_count))?;
                    let status = be_u16.parse_next(input)?;
                    let event_count = be_u16.parse_next(input)?;
                    let message_count = be_u16.parse_next(input)?;
                    let events_len =
                        usize::try_from(byte_count.saturating_sub(EVENT_LOG_FIXED_BYTES))
                            .unwrap_or(0);
                    let events = take(events_len).parse_next(input)?;
                    Self::GetCommEventLog {
                        status,
                        event_count,
                        message_count,
                        events: events.to_vec(),
                    }
                }
                FunctionCode::ReportServerId => Self::ReportServerId {
                    data: counted_bytes(input)?.to_vec(),
                },
                FunctionCode::ReadFileRecord => Self::ReadFileRecord {
                    records: file::decode_read_responses(input)?,
                },
                FunctionCode::WriteFileRecord => Self::WriteFileRecord {
                    records: file::decode_write_records(input)?,
                },
                FunctionCode::ReadFifoQueue => Self::ReadFifoQueue {
                    values: fifo_values(input)?,
                },
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
            Self::WriteMultipleCoils { address, quantity } => {
                encode_echo_quantity(15, *address, *quantity)
            }
            Self::WriteMultipleRegisters { address, quantity } => {
                encode_echo_quantity(16, *address, *quantity)
            }
            Self::MaskWriteRegister {
                address,
                and_mask,
                or_mask,
            } => {
                let mut bytes = vec![22];
                bytes.extend_from_slice(&address.to_be_bytes());
                bytes.extend_from_slice(&and_mask.to_be_bytes());
                bytes.extend_from_slice(&or_mask.to_be_bytes());
                finish(bytes)
            }
            Self::ReadWriteMultipleRegisters { registers } => {
                encode_counted(23, &registers_to_bytes(registers))
            }
            Self::ReadExceptionStatus { status } => finish(vec![7, *status]),
            Self::Diagnostics { sub_function, data } => encode_diagnostics(8, *sub_function, data),
            Self::GetCommEventCounter {
                status,
                event_count,
            } => {
                let mut bytes = vec![11];
                bytes.extend_from_slice(&status.to_be_bytes());
                bytes.extend_from_slice(&event_count.to_be_bytes());
                finish(bytes)
            }
            Self::GetCommEventLog {
                status,
                event_count,
                message_count,
                events,
            } => {
                let byte_count = EVENT_LOG_FIXED_BYTES
                    .saturating_add(u32::try_from(events.len()).unwrap_or(u32::MAX));
                check_event_log_bytes(byte_count)?;
                let mut bytes = vec![12, u8::try_from(byte_count).unwrap_or(u8::MAX)];
                bytes.extend_from_slice(&status.to_be_bytes());
                bytes.extend_from_slice(&event_count.to_be_bytes());
                bytes.extend_from_slice(&message_count.to_be_bytes());
                bytes.extend_from_slice(events);
                finish(bytes)
            }
            Self::ReportServerId { data } => encode_counted(17, data),
            Self::ReadFileRecord { records } => {
                encode_body(20, file::encode_read_responses(records)?)
            }
            Self::WriteFileRecord { records } => {
                encode_body(21, file::encode_write_records(records)?)
            }
            Self::ReadFifoQueue { values } => {
                let count = quantity_of("FIFO count", values.len(), 0, MAX_FIFO_COUNT)?;
                let mut bytes = vec![24];
                let byte_count = count * 2 + 2;
                bytes.extend_from_slice(&byte_count.to_be_bytes());
                bytes.extend_from_slice(&count.to_be_bytes());
                bytes.extend_from_slice(&registers_to_bytes(values));
                finish(bytes)
            }
            Self::Exception(exception) => exception.encode(),
        }
    }
}

/// Decode a Read FIFO Queue response body: a two-byte byte count, a FIFO count,
/// and the queued values (FR-R-041, FR-R-042).
fn fifo_values(input: &mut Input<'_>) -> ParseResult<Vec<u16>> {
    let byte_count = be_u16.parse_next(input)?;
    let count = be_u16.parse_next(input)?;
    parse::lift(check_range("FIFO count", count, 0, MAX_FIFO_COUNT))?;
    let expected = count * 2 + 2;
    if byte_count != expected {
        return parse::fail(Error::ByteCountMismatch {
            expected: usize::from(expected),
            actual: usize::from(byte_count),
        });
    }
    let data = take(usize::from(count) * 2).parse_next(input)?;
    Ok(registers_from_bytes(data))
}

/// Flatten registers to big-endian bytes (FR-R-003).
pub(super) fn registers_to_bytes(registers: &[u16]) -> Vec<u8> {
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
    /// FR-R-030 — a Write Multiple Coils request is a starting address, a
    /// quantity, a byte count of `(quantity + 7) / 8`, and that many data bytes.
    /// FR-R-034 — its response echoes the address and quantity.
    /// Bytes from the specification's example (§6.11): address 19, 10 coils.
    fn ut_write_multiple_coils_spec_example() {
        let coils = vec![
            true, false, true, true, false, false, true, true, true, false,
        ];
        let request = RequestPdu::WriteMultipleCoils {
            address: 19,
            coils: coils.clone(),
        };
        let request_bytes = vec![0x0F, 0x00, 0x13, 0x00, 0x0A, 0x02, 0xCD, 0x01];
        assert_eq!(request.encode(), Ok(request_bytes.clone()));
        assert_eq!(RequestPdu::decode(&request_bytes), Ok(request));

        let response = ResponsePdu::WriteMultipleCoils {
            address: 19,
            quantity: 10,
        };
        let response_bytes = vec![0x0F, 0x00, 0x13, 0x00, 0x0A];
        assert_eq!(response.encode(), Ok(response_bytes.clone()));
        assert_eq!(ResponsePdu::decode(&response_bytes), Ok(response));
    }

    #[test]
    /// FR-R-030 — the decoded coil count matches the quantity field, not the
    /// byte count, so the two padding bits of the example are dropped.
    fn ut_write_multiple_coils_honours_quantity() {
        let decoded = RequestPdu::decode(&[0x0F, 0x00, 0x13, 0x00, 0x0A, 0x02, 0xCD, 0x01]);
        let coils = match decoded {
            Ok(RequestPdu::WriteMultipleCoils { coils, .. }) => coils,
            _ => Vec::new(),
        };
        assert_eq!(coils.len(), 10);
    }

    #[test]
    /// FR-R-031 — a Write Multiple Coils quantity outside 1–1968 is an
    /// out-of-range error.
    fn ut_write_multiple_coils_quantity_out_of_range() {
        for quantity in [0usize, 1969, 2000] {
            let expected = Error::OutOfRange {
                field: "quantity",
                value: quantity as u32,
                min: 1,
                max: 1968,
            };
            assert_eq!(
                RequestPdu::WriteMultipleCoils {
                    address: 0,
                    coils: vec![false; quantity],
                }
                .encode(),
                Err(expected),
                "quantity {quantity}"
            );
        }
    }

    #[test]
    /// FR-R-032 — a Write Multiple Registers request is a starting address, a
    /// quantity, a byte count of `2 × quantity`, and that many data bytes.
    /// FR-R-034 — its response echoes the address and quantity.
    /// Bytes from the specification's example (§6.12): address 1, 2 registers.
    fn ut_write_multiple_registers_spec_example() {
        let request = RequestPdu::WriteMultipleRegisters {
            address: 1,
            registers: vec![0x000A, 0x0102],
        };
        let request_bytes = vec![0x10, 0x00, 0x01, 0x00, 0x02, 0x04, 0x00, 0x0A, 0x01, 0x02];
        assert_eq!(request.encode(), Ok(request_bytes.clone()));
        assert_eq!(RequestPdu::decode(&request_bytes), Ok(request));

        let response = ResponsePdu::WriteMultipleRegisters {
            address: 1,
            quantity: 2,
        };
        let response_bytes = vec![0x10, 0x00, 0x01, 0x00, 0x02];
        assert_eq!(response.encode(), Ok(response_bytes.clone()));
        assert_eq!(ResponsePdu::decode(&response_bytes), Ok(response));
    }

    #[test]
    /// FR-R-033 — a Write Multiple Registers quantity outside 1–123 is an
    /// out-of-range error.
    fn ut_write_multiple_registers_quantity_out_of_range() {
        for quantity in [0usize, 124, 125] {
            let expected = Error::OutOfRange {
                field: "quantity",
                value: quantity as u32,
                min: 1,
                max: 123,
            };
            assert_eq!(
                RequestPdu::WriteMultipleRegisters {
                    address: 0,
                    registers: vec![0; quantity],
                }
                .encode(),
                Err(expected),
                "quantity {quantity}"
            );
        }
    }

    #[test]
    /// FR-R-035 — a Mask Write Register request is a reference address, an AND
    /// mask and an OR mask, and its response echoes the request.
    /// Bytes from the specification's example (§6.16).
    fn ut_mask_write_register_spec_example() {
        let request = RequestPdu::MaskWriteRegister {
            address: 4,
            and_mask: 0x00F2,
            or_mask: 0x0025,
        };
        let bytes = vec![0x16, 0x00, 0x04, 0x00, 0xF2, 0x00, 0x25];
        assert_eq!(request.encode(), Ok(bytes.clone()));
        assert_eq!(RequestPdu::decode(&bytes), Ok(request));

        let response = ResponsePdu::MaskWriteRegister {
            address: 4,
            and_mask: 0x00F2,
            or_mask: 0x0025,
        };
        assert_eq!(response.encode(), Ok(bytes.clone()));
        assert_eq!(ResponsePdu::decode(&bytes), Ok(response));
    }

    #[test]
    /// FR-R-036 — the mask write result is
    /// `(current AND and_mask) OR (or_mask AND NOT and_mask)`. The worked
    /// example (§6.16) takes current `0x0012` to `0x0017`.
    fn ut_mask_write_result_formula() {
        assert_eq!(mask_write_result(0x0012, 0x00F2, 0x0025), 0x0017);
        // An all-ones AND mask leaves the register untouched.
        assert_eq!(mask_write_result(0xABCD, 0xFFFF, 0x1234), 0xABCD);
        // An all-zeros AND mask replaces it with the OR mask.
        assert_eq!(mask_write_result(0xABCD, 0x0000, 0x1234), 0x1234);
    }

    #[test]
    /// FR-R-037 — a Read/Write Multiple Registers request carries the read
    /// address and quantity, then the write address, quantity, byte count and
    /// data.
    /// FR-R-039 — its response is a byte count of `2 × read quantity` and that
    /// many data bytes. Bytes from the specification's example (§6.17).
    fn ut_read_write_multiple_registers_spec_example() {
        let request = RequestPdu::ReadWriteMultipleRegisters {
            read_address: 4,
            read_quantity: 6,
            write_address: 15,
            registers: vec![0x00FF, 0x00FF, 0x00FF],
        };
        let request_bytes = vec![
            0x17, 0x00, 0x04, 0x00, 0x06, 0x00, 0x0F, 0x00, 0x03, 0x06, 0x00, 0xFF, 0x00, 0xFF,
            0x00, 0xFF,
        ];
        assert_eq!(request.encode(), Ok(request_bytes.clone()));
        assert_eq!(RequestPdu::decode(&request_bytes), Ok(request));

        let response = ResponsePdu::ReadWriteMultipleRegisters {
            registers: vec![0x00FE, 0x0ACD, 0x0001, 0x0003, 0x000D, 0x00FF],
        };
        let response_bytes = vec![
            0x17, 0x0C, 0x00, 0xFE, 0x0A, 0xCD, 0x00, 0x01, 0x00, 0x03, 0x00, 0x0D, 0x00, 0xFF,
        ];
        assert_eq!(response.encode(), Ok(response_bytes.clone()));
        assert_eq!(ResponsePdu::decode(&response_bytes), Ok(response));
    }

    #[test]
    /// FR-R-038 — the read quantity is 1–125 and the write quantity 1–121.
    fn ut_read_write_multiple_registers_quantities_out_of_range() {
        assert_eq!(
            RequestPdu::ReadWriteMultipleRegisters {
                read_address: 0,
                read_quantity: 126,
                write_address: 0,
                registers: vec![0; 1],
            }
            .encode(),
            Err(Error::OutOfRange {
                field: "read quantity",
                value: 126,
                min: 1,
                max: 125,
            })
        );
        assert_eq!(
            RequestPdu::ReadWriteMultipleRegisters {
                read_address: 0,
                read_quantity: 1,
                write_address: 0,
                registers: vec![0; 122],
            }
            .encode(),
            Err(Error::OutOfRange {
                field: "write quantity",
                value: 122,
                min: 1,
                max: 121,
            })
        );
    }

    #[test]
    /// FR-R-040 — a Read FIFO Queue request is a 2-byte pointer address.
    /// FR-R-041 — its response carries a two-byte byte count, a FIFO count, and
    /// the queued values. Bytes from the specification's example (§6.18).
    fn ut_read_fifo_queue_spec_example() {
        let request = RequestPdu::ReadFifoQueue { address: 0x04DE };
        let request_bytes = vec![0x18, 0x04, 0xDE];
        assert_eq!(request.encode(), Ok(request_bytes.clone()));
        assert_eq!(RequestPdu::decode(&request_bytes), Ok(request));

        let response = ResponsePdu::ReadFifoQueue {
            values: vec![0x01B8, 0x1284],
        };
        let response_bytes = vec![0x18, 0x00, 0x06, 0x00, 0x02, 0x01, 0xB8, 0x12, 0x84];
        assert_eq!(response.encode(), Ok(response_bytes.clone()));
        assert_eq!(ResponsePdu::decode(&response_bytes), Ok(response));
    }

    #[test]
    /// FR-R-041 — the two-byte byte count equals `(2 × FIFO count) + 2`, so it
    /// counts the FIFO count field as well as the data.
    fn ut_read_fifo_queue_byte_count_includes_count_field() {
        assert_eq!(
            ResponsePdu::decode(&[0x18, 0x00, 0x04, 0x00, 0x02, 0x01, 0xB8, 0x12, 0x84]),
            Err(Error::ByteCountMismatch {
                // Two values plus the count field imply six; the wire said four.
                expected: 6,
                actual: 4,
            })
        );
    }

    #[test]
    /// FR-R-042 — a FIFO count above 31 is an out-of-range error, raised before
    /// any allocation proportional to it.
    fn ut_read_fifo_queue_count_above_31_rejected() {
        assert_eq!(
            ResponsePdu::decode(&[0x18, 0x00, 0x42, 0x00, 0x20]),
            Err(Error::OutOfRange {
                field: "FIFO count",
                value: 32,
                min: 0,
                max: 31,
            })
        );
    }

    #[test]
    /// FR-R-047 — a bit-packed request body whose padding bits above the stated
    /// quantity are not zero is an illegal-value error, since FR-R-024 requires
    /// them to be zero.
    fn ut_write_multiple_coils_rejects_nonzero_padding() {
        assert_eq!(
            RequestPdu::decode(&[0x0F, 0x00, 0x13, 0x00, 0x0A, 0x02, 0xCD, 0xFD]),
            Err(Error::IllegalValue {
                field: "coil padding",
                value: 0xFD,
            })
        );
    }

    #[test]
    /// FR-R-050 — the specification's Read File Record request example (§6.14):
    /// byte count 14, two 7-byte sub-requests.
    fn ut_read_file_record_spec_example() {
        let bytes = [
            0x14, 0x0E, 0x06, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x06, 0x00, 0x03, 0x00, 0x09,
            0x00, 0x02,
        ];
        let expected = RequestPdu::ReadFileRecord {
            records: vec![
                FileRecordRead {
                    file_number: 4,
                    record_number: 1,
                    record_length: 2,
                },
                FileRecordRead {
                    file_number: 3,
                    record_number: 9,
                    record_length: 2,
                },
            ],
        };
        assert_eq!(RequestPdu::decode(&bytes), Ok(expected.clone()));
        assert_eq!(expected.encode(), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-052 — the specification's Read File Record response example
    /// (§6.14): each sub-response's file response length is its record data
    /// byte count plus one.
    fn ut_read_file_record_response_spec_example() {
        let bytes = [
            0x14, 0x0C, 0x05, 0x06, 0x0D, 0xFE, 0x00, 0x20, 0x05, 0x06, 0x33, 0xCD, 0x00, 0x40,
        ];
        let expected = ResponsePdu::ReadFileRecord {
            records: vec![
                FileRecordReadResponse {
                    values: vec![0x0DFE, 0x0020],
                },
                FileRecordReadResponse {
                    values: vec![0x33CD, 0x0040],
                },
            ],
        };
        assert_eq!(ResponsePdu::decode(&bytes), Ok(expected.clone()));
        assert_eq!(expected.encode(), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-053 — the specification's Write File Record example (§6.15); the
    /// response is byte-for-byte the request.
    fn ut_write_file_record_spec_example() {
        let bytes = [
            0x15, 0x0D, 0x06, 0x00, 0x04, 0x00, 0x07, 0x00, 0x03, 0x06, 0xAF, 0x04, 0xBE, 0x10,
            0x0D,
        ];
        let records = vec![FileRecordWrite {
            file_number: 4,
            record_number: 7,
            values: vec![0x06AF, 0x04BE, 0x100D],
        }];
        let request = RequestPdu::WriteFileRecord {
            records: records.clone(),
        };
        let response = ResponsePdu::WriteFileRecord { records };
        assert_eq!(RequestPdu::decode(&bytes), Ok(request.clone()));
        assert_eq!(request.encode(), Ok(bytes.to_vec()));
        assert_eq!(ResponsePdu::decode(&bytes), Ok(response.clone()));
        assert_eq!(response.encode(), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-051 — a Read File Record request byte count outside 7–245 is an
    /// out-of-range error, raised before any body byte is consumed.
    fn ut_read_file_record_byte_count_out_of_range() {
        assert_eq!(
            RequestPdu::decode(&[0x14, 0x00]),
            Err(Error::OutOfRange {
                field: "request byte count",
                value: 0,
                min: 7,
                max: 245,
            })
        );
    }

    #[test]
    /// FR-R-051 — a Read File Record request byte count that is not a multiple
    /// of 7 cannot describe whole sub-requests, so it is an illegal value.
    fn ut_read_file_record_byte_count_not_multiple_of_seven() {
        assert_eq!(
            RequestPdu::decode(&[0x14, 0x08, 0x06, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x00]),
            Err(Error::IllegalValue {
                field: "request byte count",
                value: 8,
            })
        );
    }

    #[test]
    /// FR-R-055 — a reference type other than 6 is rejected wherever it appears.
    fn ut_file_record_reference_type_must_be_six() {
        assert_eq!(
            RequestPdu::decode(&[0x14, 0x07, 0x07, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02]),
            Err(Error::ReferenceType(7))
        );
        assert_eq!(
            ResponsePdu::decode(&[0x14, 0x07, 0x05, 0x05, 0x0D, 0xFE, 0x00, 0x20, 0x00]),
            Err(Error::ReferenceType(5))
        );
    }

    #[test]
    /// FR-R-057 — a file response length that is not the record data byte count
    /// plus one cannot be honoured; an even one would claim an odd number of
    /// data bytes.
    fn ut_file_response_length_must_cover_whole_registers() {
        assert_eq!(
            ResponsePdu::decode(&[0x14, 0x07, 0x06, 0x06, 0x0D, 0xFE, 0x00, 0x20, 0x00]),
            Err(Error::IllegalValue {
                field: "file response length",
                value: 6,
            })
        );
    }

    #[test]
    /// FR-R-054 — a Write File Record data length below 9 cannot hold a
    /// sub-request with any data at all.
    fn ut_write_file_record_data_length_out_of_range() {
        assert_eq!(
            RequestPdu::decode(&[0x15, 0x07, 0x06, 0x00, 0x04, 0x00, 0x07, 0x00, 0x00]),
            Err(Error::OutOfRange {
                field: "request data length",
                value: 7,
                min: 9,
                max: 251,
            })
        );
    }

    #[test]
    /// FR-R-056, FR-R-058 — file number 0 and record number 10000 are outside
    /// the ranges the specification fixes, on decode as well as encode
    /// (FR-R-133).
    fn ut_file_and_record_numbers_out_of_range() {
        let file_zero = Error::OutOfRange {
            field: "file number",
            value: 0,
            min: 1,
            max: u32::from(u16::MAX),
        };
        assert_eq!(
            RequestPdu::decode(&[0x14, 0x07, 0x06, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02]),
            Err(file_zero.clone())
        );
        assert_eq!(
            RequestPdu::ReadFileRecord {
                records: vec![FileRecordRead {
                    file_number: 0,
                    record_number: 1,
                    record_length: 2,
                }],
            }
            .encode(),
            Err(file_zero)
        );

        let record_high = Error::OutOfRange {
            field: "record number",
            value: 10_000,
            min: 0,
            max: 9_999,
        };
        assert_eq!(
            RequestPdu::decode(&[0x14, 0x07, 0x06, 0x00, 0x04, 0x27, 0x10, 0x00, 0x02]),
            Err(record_high.clone())
        );
        assert_eq!(
            RequestPdu::ReadFileRecord {
                records: vec![FileRecordRead {
                    file_number: 4,
                    record_number: 10_000,
                    record_length: 2,
                }],
            }
            .encode(),
            Err(record_high)
        );
    }

    #[test]
    /// FR-R-052, FR-R-131 — a sub-response claiming more bytes than its region
    /// holds is truncated input, measured against the stated data length.
    fn ut_file_record_sub_item_may_not_overrun_its_region() {
        assert_eq!(
            ResponsePdu::decode(&[0x14, 0x08, 0x0B, 0x06, 0x0D, 0xFE, 0x00, 0x20, 0x00, 0x00]),
            Err(Error::Truncated {
                expected: 12,
                supplied: 8,
            })
        );
    }

    #[test]
    /// FR-R-060 — the specification's Read Exception Status example (§6.7): an
    /// empty request, a single status byte in reply.
    fn ut_read_exception_status_spec_example() {
        assert_eq!(
            RequestPdu::decode(&[0x07]),
            Ok(RequestPdu::ReadExceptionStatus)
        );
        assert_eq!(RequestPdu::ReadExceptionStatus.encode(), Ok(vec![0x07]));

        let response = ResponsePdu::ReadExceptionStatus { status: 0x6D };
        assert_eq!(ResponsePdu::decode(&[0x07, 0x6D]), Ok(response.clone()));
        assert_eq!(response.encode(), Ok(vec![0x07, 0x6D]));
    }

    #[test]
    /// FR-R-061 — the specification's Diagnostics example (§6.8): Return Query
    /// Data with one data word, echoed back unchanged.
    fn ut_diagnostics_spec_example() {
        let bytes = [0x08, 0x00, 0x00, 0xA5, 0x37];
        let request = RequestPdu::Diagnostics {
            sub_function: DiagnosticSubFunction::ReturnQueryData,
            data: vec![0xA537],
        };
        let response = ResponsePdu::Diagnostics {
            sub_function: DiagnosticSubFunction::ReturnQueryData,
            data: vec![0xA537],
        };
        assert_eq!(RequestPdu::decode(&bytes), Ok(request.clone()));
        assert_eq!(request.encode(), Ok(bytes.to_vec()));
        assert_eq!(ResponsePdu::decode(&bytes), Ok(response.clone()));
        assert_eq!(response.encode(), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-061 — a Diagnostics body may carry no data words at all.
    fn ut_diagnostics_without_data_words() {
        let bytes = [0x08, 0x00, 0x0A];
        let request = RequestPdu::Diagnostics {
            sub_function: DiagnosticSubFunction::ClearCountersAndDiagnosticRegister,
            data: Vec::new(),
        };
        assert_eq!(RequestPdu::decode(&bytes), Ok(request.clone()));
        assert_eq!(request.encode(), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-061 — data words are 16 bits, so a body with an odd byte left over
    /// cannot be honoured.
    fn ut_diagnostics_odd_data_length_rejected() {
        assert_eq!(
            RequestPdu::decode(&[0x08, 0x00, 0x00, 0xA5]),
            Err(Error::IllegalValue {
                field: "diagnostic data length",
                value: 1,
            })
        );
    }

    #[test]
    /// FR-R-062 — the fifteen sub-functions the specification defines are each
    /// a named value, and each maps to its own code.
    fn ut_all_fifteen_diagnostic_sub_functions_named() {
        let named: Vec<(u16, DiagnosticSubFunction)> = vec![
            (0, DiagnosticSubFunction::ReturnQueryData),
            (1, DiagnosticSubFunction::RestartCommunicationsOption),
            (2, DiagnosticSubFunction::ReturnDiagnosticRegister),
            (3, DiagnosticSubFunction::ChangeAsciiInputDelimiter),
            (4, DiagnosticSubFunction::ForceListenOnlyMode),
            (
                10,
                DiagnosticSubFunction::ClearCountersAndDiagnosticRegister,
            ),
            (11, DiagnosticSubFunction::ReturnBusMessageCount),
            (12, DiagnosticSubFunction::ReturnBusCommunicationErrorCount),
            (13, DiagnosticSubFunction::ReturnBusExceptionErrorCount),
            (14, DiagnosticSubFunction::ReturnServerMessageCount),
            (15, DiagnosticSubFunction::ReturnServerNoResponseCount),
            (16, DiagnosticSubFunction::ReturnServerNakCount),
            (17, DiagnosticSubFunction::ReturnServerBusyCount),
            (18, DiagnosticSubFunction::ReturnBusCharacterOverrunCount),
            (20, DiagnosticSubFunction::ClearOverrunCounterAndFlag),
        ];
        assert_eq!(named.len(), 15);
        for (raw, code) in named {
            assert_eq!(DiagnosticSubFunction::decode(raw), code, "code {raw}");
            assert_eq!(code.encode(), Ok(raw), "code {raw}");
        }
    }

    #[test]
    /// FR-R-063 — an unnamed sub-function code, the reserved range 5–9
    /// included, decodes as a general value instead of failing.
    fn ut_unnamed_diagnostic_sub_function_decodes() {
        for raw in [5u16, 6, 7, 8, 9, 19, 21, 0xFFFF] {
            assert_eq!(
                DiagnosticSubFunction::decode(raw),
                DiagnosticSubFunction::Other(raw),
                "code {raw}"
            );
            assert_eq!(DiagnosticSubFunction::Other(raw).encode(), Ok(raw));
        }
    }

    #[test]
    /// FR-R-063 — a general sub-function value holding a code the crate names
    /// has two encodings, so encoding it is a reserved-code error.
    fn ut_diagnostic_other_holding_named_code_is_reserved_error() {
        assert_eq!(
            DiagnosticSubFunction::Other(11).encode(),
            Err(Error::ReservedCode(11))
        );
    }

    #[test]
    /// FR-R-064 — the specification's Get Comm Event Counter example (§6.9): an
    /// empty request; status `0xFFFF` and event count `0x0108` in reply.
    fn ut_get_comm_event_counter_spec_example() {
        assert_eq!(
            RequestPdu::decode(&[0x0B]),
            Ok(RequestPdu::GetCommEventCounter)
        );
        assert_eq!(RequestPdu::GetCommEventCounter.encode(), Ok(vec![0x0B]));

        let bytes = [0x0B, 0xFF, 0xFF, 0x01, 0x08];
        let response = ResponsePdu::GetCommEventCounter {
            status: 0xFFFF,
            event_count: 0x0108,
        };
        assert_eq!(ResponsePdu::decode(&bytes), Ok(response.clone()));
        assert_eq!(response.encode(), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-068 — a status word that is neither `0x0000` nor `0xFFFF` is
    /// carried as it stands, not rejected.
    fn ut_comm_event_status_word_carried_as_is() {
        assert_eq!(
            ResponsePdu::decode(&[0x0B, 0x12, 0x34, 0x00, 0x01]),
            Ok(ResponsePdu::GetCommEventCounter {
                status: 0x1234,
                event_count: 1,
            })
        );
    }

    #[test]
    /// FR-R-065 — the specification's Get Comm Event Log example (§6.10): byte
    /// count 8 covering two event bytes plus the six fixed ones.
    fn ut_get_comm_event_log_spec_example() {
        assert_eq!(RequestPdu::decode(&[0x0C]), Ok(RequestPdu::GetCommEventLog));
        assert_eq!(RequestPdu::GetCommEventLog.encode(), Ok(vec![0x0C]));

        let bytes = [0x0C, 0x08, 0x00, 0x00, 0x01, 0x08, 0x01, 0x21, 0x20, 0x00];
        let response = ResponsePdu::GetCommEventLog {
            status: 0x0000,
            event_count: 0x0108,
            message_count: 0x0121,
            events: vec![0x20, 0x00],
        };
        assert_eq!(ResponsePdu::decode(&bytes), Ok(response.clone()));
        assert_eq!(response.encode(), Ok(bytes.to_vec()));
    }

    #[test]
    /// FR-R-065 — the byte count covers six fixed bytes plus 0–64 event bytes,
    /// so it is out of range below 6 or above 70.
    fn ut_comm_event_log_byte_count_out_of_range() {
        assert_eq!(
            ResponsePdu::decode(&[0x0C, 0x05, 0x00, 0x00, 0x01, 0x08, 0x01]),
            Err(Error::OutOfRange {
                field: "byte count",
                value: 5,
                min: 6,
                max: 70,
            })
        );
    }

    #[test]
    /// FR-R-066 — the Report Server ID body is device-specific, so it is
    /// carried whole behind its byte count. The bytes are synthetic: the
    /// specification gives no worked example for a device-specific reply.
    fn ut_report_server_id_body_is_opaque() {
        assert_eq!(RequestPdu::decode(&[0x11]), Ok(RequestPdu::ReportServerId));
        assert_eq!(RequestPdu::ReportServerId.encode(), Ok(vec![0x11]));

        let bytes = [0x11, 0x03, 0xAA, 0xFF, 0x01];
        let response = ResponsePdu::ReportServerId {
            data: vec![0xAA, 0xFF, 0x01],
        };
        assert_eq!(ResponsePdu::decode(&bytes), Ok(response.clone()));
        assert_eq!(response.encode(), Ok(bytes.to_vec()));
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
            vec![0x0F, 0x00, 0x13, 0x00, 0x0A, 0x02, 0xCD, 0x01],
            vec![0x10, 0x00, 0x01, 0x00, 0x02, 0x04, 0x00, 0x0A, 0x01, 0x02],
            vec![0x16, 0x00, 0x04, 0x00, 0xF2, 0x00, 0x25],
            vec![
                0x17, 0x00, 0x04, 0x00, 0x06, 0x00, 0x0F, 0x00, 0x03, 0x06, 0x00, 0xFF, 0x00, 0xFF,
                0x00, 0xFF,
            ],
            vec![0x18, 0x04, 0xDE],
            vec![0x07],
            vec![0x08, 0x00, 0x00, 0xA5, 0x37],
            vec![0x0B],
            vec![0x0C],
            vec![0x11],
            vec![
                0x14, 0x0E, 0x06, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x06, 0x00, 0x03, 0x00, 0x09,
                0x00, 0x02,
            ],
            vec![
                0x15, 0x0D, 0x06, 0x00, 0x04, 0x00, 0x07, 0x00, 0x03, 0x06, 0xAF, 0x04, 0xBE, 0x10,
                0x0D,
            ],
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
            vec![0x0F, 0x00, 0x13, 0x00, 0x0A],
            vec![0x10, 0x00, 0x01, 0x00, 0x02],
            vec![0x16, 0x00, 0x04, 0x00, 0xF2, 0x00, 0x25],
            vec![
                0x17, 0x0C, 0x00, 0xFE, 0x0A, 0xCD, 0x00, 0x01, 0x00, 0x03, 0x00, 0x0D, 0x00, 0xFF,
            ],
            vec![0x18, 0x00, 0x06, 0x00, 0x02, 0x01, 0xB8, 0x12, 0x84],
            vec![0x07, 0x6D],
            vec![0x08, 0x00, 0x00, 0xA5, 0x37],
            vec![0x0B, 0xFF, 0xFF, 0x01, 0x08],
            vec![0x0C, 0x08, 0x00, 0x00, 0x01, 0x08, 0x01, 0x21, 0x20, 0x00],
            vec![0x11, 0x03, 0xAA, 0xFF, 0x01],
            vec![
                0x14, 0x0C, 0x05, 0x06, 0x0D, 0xFE, 0x00, 0x20, 0x05, 0x06, 0x33, 0xCD, 0x00, 0x40,
            ],
            vec![
                0x15, 0x0D, 0x06, 0x00, 0x04, 0x00, 0x07, 0x00, 0x03, 0x06, 0xAF, 0x04, 0xBE, 0x10,
                0x0D,
            ],
        ];
        for bytes in responses {
            let decoded = ResponsePdu::decode(&bytes).expect("valid response");
            assert_eq!(decoded.encode(), Ok(bytes.clone()), "response {bytes:?}");
        }
    }
}
