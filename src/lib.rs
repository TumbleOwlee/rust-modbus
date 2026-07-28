//! Async Modbus client and server over RTU and TCP.
//!
//! The authoritative specification of this crate's behavior lives in
//! `docs/specs/`; requirement IDs cited in doc comments (`FR-R-*`, …) refer to
//! it.

#![forbid(unsafe_code)]

mod error;
mod frame;
mod parse;

pub use error::{Error, Result};
pub use frame::{
    ExceptionCode, ExceptionResponse, FunctionCode, MAX_PDU_LEN, RequestPdu, ResponsePdu,
};
