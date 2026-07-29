//! Async Modbus client and server over RTU and TCP.
//!
//! The authoritative specification of this crate's behavior lives in
//! `docs/specs/`; requirement IDs cited in doc comments (`FR-R-*`, …) refer to
//! it.

#![forbid(unsafe_code)]
// NF-R-001, NF-R-002: `core` + `alloc` always; `std` only where the transport,
// client, and server areas need it.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod error;
mod frame;
mod parse;
#[cfg(feature = "std")]
mod transport;

pub use error::{Error, Result};
pub use frame::{
    Address, AduBoundary, Ascii, DeviceIdObject, DiagnosticSubFunction, ExceptionCode,
    ExceptionResponse, ExceptionStatus, FileNumber, FileRecordRead, FileRecordReadResponse,
    FileRecordWrite, Framing, FunctionCode, MAX_PDU_LEN, Mask, MbapHeader, MeiRequest, MeiResponse,
    Quantity, ReadDeviceIdCode, RecordLength, RecordNumber, RegisterValue, RequestPdu, ResponsePdu,
    Rtu, Tcp, TransactionId, UnitId, mask_write_result,
};
#[cfg(feature = "std")]
pub use transport::{
    DataBits, FlowControl, FrameTransport, Parity, SerialConfig, StopBits, TcpConfig, TcpListener,
    TcpTransport, TransportConfig, connect_tcp,
};
#[cfg(feature = "rtu")]
pub use transport::{SerialTransport, open_serial};
