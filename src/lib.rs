//! Async Modbus client and server over RTU and TCP.
//!
//! The crate covers all four combinations of role and transport — client and
//! server, over Modbus TCP and over Modbus RTU serial — on one shared frame
//! layer, so the two roles cannot drift apart. Modbus ASCII framing is
//! encodable and decodable too, though only as a frame format: this crate does
//! not operate a serial port in ASCII mode.
//!
//! The three layers are worth knowing apart, because each is usable on its own:
//!
//! - [`RequestPdu`] / [`ResponsePdu`] and the [`Framing`] implementations
//!   ([`Rtu`], [`Ascii`], [`Tcp`]) are pure encoding and decoding. No I/O, no
//!   `std`, no runtime.
//! - `FrameTransport` pairs a framing with an async byte stream and reads or
//!   writes whole ADUs, finding each frame's boundary for you.
//! - `Client` and `Server` are the two roles on top of that: one issues
//!   requests and matches replies, the other accepts them and hands each to
//!   your `Service`.
//!
//! # Features
//!
//! | Feature | Default | What it gates |
//! |---|---|---|
//! | `std` | **on** | Everything but the frame layer — `Client`, `Server`, `FrameTransport`. Pulls in Tokio. |
//! | `rtu` | off | Opening a real serial port (`open_serial`, `SerialTransport`, [`SerialStream`], `RtuClient`, `AsciiClient`). Implies `std`. |
//!
//! Turning `std` off leaves a `no_std` + `alloc` crate that still encodes and
//! decodes every function code over every framing. Turning `rtu` on is only
//! needed to open a port: because `Client` and `Server` are generic over
//! the stream, `Rtu` framing over any duplex stream — an in-memory pipe, a
//! socket, a pty — works with the feature off.
//!
//! # Encoding and decoding a frame
//!
//! No transport, no runtime, no feature flags. This is the layer the `no_std`
//! build keeps.
//!
//! ```
//! use rust_modbus::{
//!     Address, Framing, MbapHeader, Quantity, RegisterValue, RequestPdu, ResponsePdu, Tcp,
//!     TransactionId, UnitId,
//! };
//!
//! // Read one holding register at address 0x006B from unit 1.
//! let header = MbapHeader { transaction_id: TransactionId(1), unit_id: UnitId(1) };
//! let request = RequestPdu::ReadHoldingRegisters {
//!     address: Address(0x006B),
//!     quantity: Quantity(1),
//! };
//!
//! // A Modbus TCP ADU is an MBAP header followed by the PDU. The bytes below
//! // are the wire format: transaction 1, protocol 0, length 6, unit 1, then
//! // function 0x03 with a big-endian address and quantity.
//! let adu = Tcp::encode_request(&header, &request)?;
//! assert_eq!(adu, [0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x6B, 0x00, 0x01]);
//!
//! // Decoding is direction-explicit: a PDU does not say whether it is a
//! // request or a reply, so you say which you are holding.
//! let (decoded_header, decoded) = Tcp::decode_request(&adu)?;
//! assert_eq!((decoded_header, decoded), (header, request));
//!
//! // The reply to that request, as a device would send it.
//! let reply = Tcp::encode_response(
//!     &header,
//!     &ResponsePdu::ReadHoldingRegisters { registers: vec![RegisterValue(0x022B)] },
//! )?;
//! assert_eq!(reply, [0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03, 0x02, 0x02, 0x2B]);
//! # Ok::<(), rust_modbus::Error>(())
//! ```
//!
//! # A client and a server, end to end
//!
//! Both roles at once over an in-memory duplex pair, so the example needs no
//! network and no device. Swap `tokio::io::duplex` for `connect_tcp` and
//! `Server::serve` and it is a real Modbus TCP exchange; see the `examples/`
//! directory for that shape.
//!
#![cfg_attr(feature = "std", doc = "```")]
#![cfg_attr(not(feature = "std"), doc = "```ignore")]
//! use std::collections::HashMap;
//! use std::sync::{Arc, Mutex};
//!
//! use rust_modbus::{
//!     Address, Client, Connection, ExceptionCode, FrameTransport, Quantity, RegisterValue,
//!     RequestPdu, ResponsePdu, Server, Service, Tcp, UnitId,
//! };
//!
//! // The crate ships no data model on purpose (SV-R-005), so a server is your
//! // own type answering requests. `&self`, not `&mut self`: every connection
//! // shares one service, so mutable state goes behind your own lock.
//! #[derive(Clone, Default)]
//! struct Plc {
//!     holding: Arc<Mutex<HashMap<u16, u16>>>,
//! }
//!
//! impl Service for Plc {
//!     async fn on_request(
//!         &self,
//!         _conn: &Connection,
//!         _unit: UnitId,
//!         request: RequestPdu,
//!     ) -> Result<ResponsePdu, ExceptionCode> {
//!         let mut holding = self.holding.lock().expect("no doctest poisons the lock");
//!         match request {
//!             RequestPdu::WriteSingleRegister { address, value } => {
//!                 holding.insert(address.0, value.0);
//!                 // The response echoes the request, as the protocol requires.
//!                 Ok(ResponsePdu::WriteSingleRegister { address, value })
//!             }
//!             RequestPdu::ReadHoldingRegisters { address, quantity } => {
//!                 let registers = (0..quantity.0)
//!                     .map(|offset| {
//!                         let at = address.0.checked_add(offset)
//!                             .ok_or(ExceptionCode::IllegalDataAddress)?;
//!                         // A refusal is stated in Modbus' own vocabulary: the
//!                         // client sees an exception response, not an I/O error.
//!                         holding.get(&at).copied().map(RegisterValue)
//!                             .ok_or(ExceptionCode::IllegalDataAddress)
//!                     })
//!                     .collect::<Result<Vec<_>, _>>()?;
//!                 Ok(ResponsePdu::ReadHoldingRegisters { registers })
//!             }
//!             // Anything this device does not implement.
//!             _ => Err(ExceptionCode::IllegalFunction),
//!         }
//!     }
//! }
//!
//! # async fn run() -> rust_modbus::Result<()> {
//! let (server_end, client_end) = tokio::io::duplex(512);
//!
//! // `serve_link` runs one already-established stream, which is also how a
//! // serial line is served. `serve` is the accept loop for a TCP listener.
//! let plc = Plc::default();
//! let server = Server::new(plc.clone());
//! let handle = server.handle();
//! let serving = tokio::spawn(server.serve_link(FrameTransport::<_, Tcp>::new(server_end)));
//!
//! // `&mut self` on every request method is how one-request-at-a-time is
//! // enforced: the borrow checker does it, with no runtime flag to check.
//! let mut client: Client<_, Tcp> = Client::new(FrameTransport::new(client_end));
//! client.write_single_register(UnitId(1), Address(7), RegisterValue(0x1234)).await?;
//! let read = client.read_holding_registers(UnitId(1), Address(7), Quantity(1)).await?;
//! assert_eq!(read, vec![RegisterValue(0x1234)]);
//!
//! // The service kept its own view of its own state, because it shares when cloned.
//! assert_eq!(plc.holding.lock().expect("not poisoned").get(&7), Some(&0x1234));
//!
//! // `shutdown` returns once every in-flight handler has finished.
//! handle.shutdown().await;
//! let _ = serving.await;
//! # Ok(())
//! # }
//! # tokio::runtime::Builder::new_current_thread()
//! #     .enable_all()
//! #     .build()
//! #     .unwrap()
//! #     .block_on(run())
//! #     .unwrap();
//! ```
//!
//! # Errors
//!
//! Every failure is a variant of [`Error`] — never a formatted string a caller
//! has to match on by substring. Bytes from a peer never panic the decoder:
//! truncated, malformed, or hostile input produces an error.
//!
//! A device's *refusal* is not an I/O failure and does not share a channel with
//! one. A client sees `Error::Exception`, carrying the function code it asked
//! about and the [`ExceptionCode`] the device answered with.
//!
//! # Further reading
//!
//! The authoritative specification of this crate's behavior lives in
//! `docs/specs/`; requirement IDs cited in doc comments (`FR-R-*`, `CL-R-*`,
//! `SV-R-*`, `TR-R-*`, `NF-R-*`) refer to it. `PRD.md` states what the library
//! is and is not for, and `ARCHITECTURE.md` maps the modules.

// NF-R-011. With `rs485` off, no `unsafe` anywhere, enforced by the compiler
// rather than by review. Enabling `rs485` narrows this to `deny`, admitting
// exactly the one documented `#[allow(unsafe_code)]` block that issues the
// `TIOCSRS485` ioctl (TR-R-055) — every other unsafe block, in this crate
// today or added later, still fails the build.
#![cfg_attr(not(feature = "rs485"), forbid(unsafe_code))]
#![cfg_attr(feature = "rs485", deny(unsafe_code))]
#![warn(missing_docs)]
// NF-R-001, NF-R-002: `core` + `alloc` always; `std` only where the transport,
// client, and server areas need it.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
mod client;
mod error;
mod frame;
mod parse;
#[cfg(feature = "std")]
mod server;
#[cfg(feature = "std")]
mod transport;

#[cfg(feature = "rtu")]
pub use client::{AsciiClient, RtuClient};
#[cfg(feature = "std")]
pub use client::{
    Client, ClientConfig, ClientFraming, ClientState, CommEventCounter, CommEventLog,
    RtuOverTcpClient, TcpClient, UnusableReason,
};
#[cfg(all(feature = "sync", feature = "rtu"))]
pub use client::{SyncAsciiClient, SyncRtuClient};
#[cfg(feature = "sync")]
pub use client::{SyncClient, SyncRtuOverTcpClient, SyncTcpClient};
pub use error::{Error, Result};
pub use frame::{
    Address, AduBoundary, Ascii, DeviceIdObject, DiagnosticSubFunction, Direction, ExceptionCode,
    ExceptionResponse, ExceptionStatus, Extent, FileNumber, FileRecordRead, FileRecordReadResponse,
    FileRecordWrite, Framing, FunctionCode, MAX_PDU_LEN, Mask, MbapHeader, MeiRequest, MeiResponse,
    Quantity, ReadDeviceIdCode, RecordLength, RecordNumber, RegisterValue, RequestPdu, ResponsePdu,
    Rtu, RtuOverTcp, Tcp, TransactionId, UnitId, mask_write_result,
};
#[cfg(feature = "std")]
pub use server::{
    Acceptance, Connection, ConnectionId, Disconnect, Server, ServerConfig, ServerFraming,
    ServerHandle, Service,
};
#[cfg(feature = "tls")]
pub use transport::{
    ClientCertPolicy, ClientIdentity, MODBUS_TLS_PORT, RootStore, ServerCertVerification,
    TlsClientConfig, TlsClientTransport, TlsListener, TlsServerConfig, connect_tls,
    connect_tls_framed, load_pem_cert_chain, load_pem_private_key,
};
#[cfg(feature = "std")]
pub use transport::{
    DataBits, FlowControl, FrameTransport, Parity, RtuOverTcpTransport, SerialConfig, StopBits,
    TcpConfig, TcpListener, TcpTransport, TransportConfig, connect_tcp, connect_tcp_framed,
};
#[cfg(feature = "rs485")]
pub use transport::{Rs485Config, RtsPolarity};
#[cfg(feature = "rtu")]
pub use transport::{SerialStream, SerialTransport, open_serial};
