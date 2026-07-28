//! Async Modbus client and server over RTU and TCP.
//!
//! The authoritative specification of this crate's behavior lives in
//! `docs/specs/`; requirement IDs cited in doc comments (`FR-R-*`, …) refer to
//! it.

#![forbid(unsafe_code)]

mod error;
// The parsing primitives have no non-test caller until the first decoder lands
// in the next stage. This allow goes away with it.
#[allow(dead_code)]
mod parse;

pub use error::{Error, Result};
