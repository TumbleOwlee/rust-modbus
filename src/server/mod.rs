//! Async Modbus server (responder). See `docs/specs/server/`.

mod service;

pub use service::{Connection, ConnectionId, Disconnect, Service};
