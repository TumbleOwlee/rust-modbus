//! Modbus PDU and ADU encoding and decoding.
//!
//! Role-agnostic: everything here is true of a byte sequence regardless of
//! whether a client or a server produced it. See `docs/specs/frame/`.

mod exception;
mod file;
mod function;
mod pdu;

pub use exception::{ExceptionCode, ExceptionResponse};
pub use file::{FileRecordRead, FileRecordReadResponse, FileRecordWrite};
pub use function::FunctionCode;
pub use pdu::{MAX_PDU_LEN, RequestPdu, ResponsePdu, mask_write_result};
