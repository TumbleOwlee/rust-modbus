//! Async Modbus client and server over RTU and TCP.
//!
//! The authoritative specification of this crate's behavior lives in
//! `docs/specs/`; requirement IDs cited in doc comments (`FR-R-*`, …) refer to
//! it.

#![forbid(unsafe_code)]
#![no_std]

extern crate alloc;

mod error;
mod frame;
mod parse;

pub use error::{Error, Result};
pub use frame::{
    DeviceIdObject, DiagnosticSubFunction, ExceptionCode, ExceptionResponse, FileRecordRead,
    FileRecordReadResponse, FileRecordWrite, FunctionCode, MAX_PDU_LEN, MeiRequest, MeiResponse,
    ReadDeviceIdCode, RequestPdu, ResponsePdu, mask_write_result,
};
