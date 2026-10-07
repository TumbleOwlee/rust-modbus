# Changelog

All notable changes to `rust-modbus` are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). While the major version is 0, a breaking change bumps the *minor* version and an additive or fixing change bumps the *patch* version. Public enums and structs are exhaustive, so adding an error variant, an enum variant, or a configuration field is a breaking change.

## [Unreleased]

### Added

- `Service::on_accept_error` and `AcceptErrorAction` (`Continue` / `Stop`): a service can now keep `serve`, `serve_framed` and `serve_tls` running after a failed `accept()`. On `Continue`, live connections are kept and accepting resumes once the hook's future completes, so the hook can back off by awaiting. The default answer is `Stop`, which drains live connections and returns the error as before.

### Changed

- **Breaking:** the futures returned by `ClientTransport::send_request` and `recv_response` are now `Send`, so a `Client::call` future awaited generically over `T: ClientTransport<F>` can be handed to `tokio::spawn`. The `FrameTransport`, `UdpTransport` and `SyncClient` implementations require `F: Send` and `F::Header: Sync` where needed; a custom `ClientTransport` implementation must return `Send` futures.
- **Breaking:** `Server::serve_link` returns `Err(error)` when its link ends with `Disconnect::Failed(error)`, instead of `Ok(())`. It still returns `Ok(())` when the link is closed by the peer, refused, or shut down, and returns only after `on_disconnect` has completed.

## [0.2.0] - 2026-08-26

### Added

- ASCII as a full serial operating mode: `TransportConfig::ascii_inter_character_timeout` (default 1 s, not derived from the baud rate). A frame stalled mid-way fails with `Error::Timeout { what: "ascii inter-character" }` and the next receive starts clean.
- Pipelined clients behind the off-by-default `pipeline` feature (implies `std`): `PipelinedClient` (TCP) and `PipelinedUdpClient` (UDP) keep several requests in flight on one connection, matched by MBAP transaction id. The handle is cloneable; the transport closes when the last handle is dropped. `PipelineConfig` (convertible from `ClientConfig`) sets the in-flight limit; `send` waits for a free slot, `try_send` fails at once with the new `Error::TooManyInFlight`. RTU and ASCII are served only by `Client`.
- `Service::on_request` may answer `Ok(None)` for any unit id: no response is sent and the connection continues, as for a broadcast.
- `AduReader`: a read-only frame reader for a stream offering only `AsyncRead` (a listen-only serial port, or bytes replayed from a capture), yielding one undecoded ADU per call.
- `crc16` and `lrc` as public functions, to validate a captured RTU or ASCII frame independently of decoding it.
- `ClientCertPolicy::AllowAny`: require a TLS client certificate without validating its chain or identity.

### Changed

- **Breaking:** `Service::on_request` returns `Result<Option<ResponsePdu>, ExceptionCode>` instead of `Result<ResponsePdu, ExceptionCode>`.
- **Breaking:** `TransportConfig` gained the `ascii_inter_character_timeout` field, `Error` the `TooManyInFlight` variant (with `pipeline`), and `ClientCertPolicy` the `AllowAny` variant.

## [0.1.0] - 2026-08-08

First release.

### Added

- Frame layer: PDU and ADU encode/decode for function codes 1–8, 11, 12, 15–17, 20–24 and 43 (MEI 14), custom function codes, and exception responses; RTU with CRC-16, TCP with the MBAP header, and ASCII with LRC. Builds for `no_std` targets with `core` + `alloc`.
- Typed domain values (`UnitId`, `Address`, `Quantity`, `RegisterValue`, `TransactionId`, and others) that cannot be mixed up at compile time, with `Display` showing the bare value. `FunctionCode` and `ExceptionCode` display their English names.
- Allocation-free sending: `encode_into` on PDUs and `Framing` writes into a caller-supplied buffer, and a transport reuses one outgoing buffer.
- Async client over TCP, RTU, ASCII and UDP (`TcpClient`, `RtuClient`, `AsciiClient`, `RtuOverTcpClient`, `UdpClient`), generic over any `ClientTransport`: response matching, configurable timeouts, typed data-access methods, broadcast handling, and `Client::state` reporting whether the client is still usable and why not.
- Blocking client behind the `sync` feature: `SyncClient` and the `SyncTcpClient`, `SyncRtuOverTcpClient`, `SyncRtuClient` and `SyncAsciiClient` aliases, with no runtime needed by the caller.
- Async server: a `Service` trait, a task per connection, unit-id filtering, broadcast handling, connection lifecycle notifications with `Acceptance` and `Disconnect`, and a `ServerHandle` that shuts down with a drain. The crate ships no register tables; the data model is yours.
- Transports: TCP (`connect_tcp`, `TcpListener`), serial RTU and ASCII behind the `rtu` feature (`open_serial`, `SerialConfig`, `SerialStream` re-exported), UDP (`connect_udp`, `Server::serve_udp`), and RTU framing over TCP for transparent serial gateways (`RtuOverTcp`, `connect_tcp_framed`, `TcpListener::accept_framed`, `Server::serve_framed`).
- A corrupted RTU or ASCII frame costs one frame, not the link; on TCP a frame that cannot be decoded ends the connection.
- RS-485 kernel direction control on Linux behind the `rs485` feature: `SerialConfig::rs485`, `Rs485Config`, `RtsPolarity`, and `Error::Rs485Unsupported` when the platform or driver refuses it.
- Modbus/TCP Security behind the `tls` feature (`rustls`): `connect_tls`, `TlsListener`, `Server::serve_tls`, server and client certificate verification (`ServerCertVerification`, `ClientCertPolicy`), `Connection::peer_cert`, `Service::on_tls_handshake_failed`, PEM-loading helpers, and `Error::TlsHandshake` carrying the `rustls` error and any rejected client certificate.
- `serde` support behind the `serde` feature for the domain values and every configuration type.
- Minimum supported Rust version 1.88.
- Hostile, truncated or oversized peer input produces a typed error, never a panic or unbounded allocation. No `unsafe` code outside the `rs485` ioctl.

[Unreleased]: https://github.com/TumbleOwlee/rust-modbus/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/TumbleOwlee/rust-modbus/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/TumbleOwlee/rust-modbus/releases/tag/v0.1.0
