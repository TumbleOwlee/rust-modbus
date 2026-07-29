//! Byte-level transports: sockets, serial ports, and ADU boundaries
//! (`TR-R-*`).
//!
//! Role-agnostic: a client and a server use the same types, differing only in
//! which direction they send and which they receive.

mod serial;

pub use serial::{DataBits, FlowControl, Parity, SerialConfig, StopBits};
