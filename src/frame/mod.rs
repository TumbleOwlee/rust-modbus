//! Modbus PDU and ADU encoding and decoding.
//!
//! Role-agnostic: everything here is true of a byte sequence regardless of
//! whether a client or a server produced it. See `docs/specs/frame/`.

mod ascii;
mod diagnostics;
mod exception;
mod file;
mod framing;
mod function;
mod mei;
mod pdu;
mod rtu;
mod tcp;
mod value;

pub use ascii::Ascii;
pub use diagnostics::DiagnosticSubFunction;
pub use exception::{ExceptionCode, ExceptionResponse};
pub use file::{FileRecordRead, FileRecordReadResponse, FileRecordWrite};
pub use framing::{AduBoundary, Framing};
pub use function::FunctionCode;
pub use mei::{DeviceIdObject, MeiRequest, MeiResponse, ReadDeviceIdCode};
pub use pdu::{MAX_PDU_LEN, RequestPdu, ResponsePdu, mask_write_result};
pub use rtu::Rtu;
pub use tcp::{MbapHeader, Tcp};
pub(crate) use value::BROADCAST_UNIT;
pub use value::{
    Address, ExceptionStatus, FileNumber, Mask, Quantity, RecordLength, RecordNumber,
    RegisterValue, TransactionId, UnitId,
};
