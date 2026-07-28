//! Modbus PDU and ADU encoding and decoding.
//!
//! Role-agnostic: everything here is true of a byte sequence regardless of
//! whether a client or a server produced it. See `docs/specs/frame/`.

mod function;

pub use function::FunctionCode;
