# Changelog

All notable changes to `rust-modbus` are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(NF-R-016). While the major version is 0, a breaking change bumps the *minor*
version and an additive or fixing change bumps the *patch* version.

This file is required by **NF-R-019**: every release records its added, changed,
and removed public API, every breaking change, and every MSRV change. What counts
as breaking is enumerated in NF-R-017 — note that this crate's public enums and
structs are exhaustive, so adding an error variant or a configuration field is a
breaking change, not an additive one.

## [Unreleased]

Nothing released yet. `0.1.0` is in development; the entries below accumulate
until it ships.

### Added

- Frame layer (`FR-R-*`): PDU and ADU encode/decode for the supported function
  codes, exception responses, RTU CRC-16, the TCP MBAP header, and Modbus ASCII
  framing. `core` + `alloc` only, so it builds for `no_std` targets.
- Async client (`CL-R-*`) over TCP, RTU, and ASCII framing, with response
  matching, configurable timeouts, and typed data-access methods.
- Async server (`SV-R-*`): a `Service` trait, per-connection tasks, unit-id
  filtering, broadcast handling, and a shutdown handle with a drain. The crate
  ships no register tables — the data model is the consumer's (SV-R-005).
- Transport layer (`TR-R-*`): TCP sockets and RTU serial ports behind a common
  framing-aware transport seam, with ADU-bounded read buffering.
- Feature flags `std` (default, NF-R-002) and `rtu` (optional, TR-R-032).
- Declared MSRV of 1.88.0 (`rust-version`, NF-R-005), verified by CI on that
  exact toolchain rather than merely asserted.
- Supply-chain and licence audit in CI via `cargo-deny` (NF-R-015); the
  permissive allow-list and the reasoning for each non-standard licence in the
  tree live in `deny.toml`.

- `Client::state`, `ClientState`, and `UnusableReason`: what a client knows about
  its own usability, including why it became unusable (CL-R-034 … CL-R-038).

- A `README.md`, crate-level documentation with runnable doctests, and three
  examples: `tcp_client`, `rtu_client` (needs the `rtu` feature and hardware),
  and `interop_server`.

- Appending encode throughout the frame layer: `RequestPdu::encode_into`,
  `ResponsePdu::encode_into`, and `Framing::{encode_request_into,
  encode_response_into}` write into a caller-supplied buffer instead of
  returning a new one (FR-R-140 … FR-R-143). The allocating `encode` forms
  remain, defined in terms of the appending ones. A transport now owns and
  reuses a single outgoing buffer (TR-R-043), so sending in steady state
  performs no allocation at all (NF-R-009) — asserted by counting allocator
  calls in `tests/allocation.rs`, not merely stated.

- Modbus RTU framing over a TCP socket, for transparent serial gateways
  (FR-R-145 … FR-R-150, TR-R-024, TR-R-033, TR-R-045, TR-R-046, TR-R-048,
  SV-R-053). `RtuOverTcp` carries the RTU ADU byte for byte and differs only in
  where a frame ends: the extent is derived from the direction, the function
  code, and the frame's own byte-count fields, since a socket has no inter-frame
  silence to observe. New public items: `RtuOverTcp`, `Direction`, `Extent`,
  `Error::IndeterminateLength`, `AduBoundary::ContentLength`,
  `RtuOverTcpTransport`, `RtuOverTcpClient`, `connect_tcp_framed`,
  `TcpListener::accept_framed`, and `Server::serve_framed`. Function code 8,
  function code 43 outside MEI type 14, and custom codes are refused with
  `IndeterminateLength` rather than misdelimited, and the boundary is not
  self-locating, so a bad frame costs the connection (FR-R-150).

- RS-485 kernel direction control on Linux, behind the off-by-default `rs485`
  feature (implies `rtu`): `SerialConfig.rs485: Option<Rs485Config>`,
  `Rs485Config`, `RtsPolarity`, and `Error::Rs485Unsupported` (TR-R-050 …
  TR-R-057). `open_serial` issues the `TIOCSRS485` ioctl after the port opens
  and before the transport is returned, so a caller never holds a transport
  whose direction control silently failed to apply; off Linux, or when the
  driver refuses the ioctl, `open_serial` fails with `Rs485Unsupported` rather
  than the port. No application-driven GPIO hook — direction control is
  delegated entirely to the kernel driver, and the after-send RTS level is
  always the on-send level's complement. This is the crate's only unsafe
  code, admitted by narrowing `forbid(unsafe_code)` to `deny(unsafe_code)`
  when `rs485` is enabled (NF-R-011); every other build configuration still
  forbids it outright.

- `core::fmt::Display` for the ten domain value types of FR-R-007 (unadorned
  wrapped value, e.g. `UnitId(17)` renders `"17"`), for `FunctionCode` (English
  name, e.g. `"Read Holding Registers"`, or `"Custom function <n>"`), and for
  `ExceptionCode` (English name, or `"Other exception <n>"`) — unconditional,
  not feature-gated (FR-R-152, FR-R-153, FR-R-154).

- An off-by-default `serde` feature (NF-R-025), `default-features = false`
  with only `derive` and `alloc`. `Serialize`/`Deserialize` for the ten domain
  value types as `#[serde(transparent)]` (FR-R-151), and for `ClientConfig`,
  `ServerConfig`, `SerialConfig`, `TcpConfig`, `TransportConfig`,
  `Rs485Config` (with `rs485`), and the serial enums `DataBits`, `Parity`,
  `StopBits`, `FlowControl`, `RtsPolarity` (CL-R-065, SV-R-054, TR-R-058,
  TR-R-059). `Duration` fields keep `Duration`'s own representation
  (`{secs, nanos}`) rather than a count in one unit, so every value a caller
  can construct round-trips exactly: the 19200-8E1 default interval of
  2,005,208 ns, a sub-millisecond timeout, and a duration whose nanosecond
  count would overflow an integer field alike. A single-unit representation
  would have been tidier in a config file at the cost of rounding the first
  two and failing on the third. The field names are a compatibility surface
  from here on. A deserialized `SerialConfig` with a zero baud rate is
  accepted exactly as direct construction accepts it — the configuration error
  fires on first use, not at deserialize time.

### Changed

- `AduBoundary` gained a `ContentLength` variant. The enum is exhaustive, so a
  `match` on it outside this crate must handle the new variant (NF-R-017).

- `Framing`'s required methods are now `encode_request_into` and
  `encode_response_into`; `encode_request` and `encode_response` became provided
  methods. An implementor of the trait outside this crate must implement the
  appending pair instead of the allocating one.

- A corrupted RTU or ASCII frame now costs exactly one frame instead of the
  link. Both framings delimit their frames on the wire — RTU by silence, ASCII
  by `:` and CRLF — so the next boundary survives a frame that fails to decode,
  and a client stays synchronized while a server stays on the bus (FR-R-144,
  CL-R-023, SV-R-050, TR-R-044). TCP is unchanged: its length prefix is carried
  by the frame itself, so a frame that cannot be decoded takes the stream's
  alignment with it. `AduBoundary::is_self_locating` reports which of the two a
  framing is.

### Fixed

- The `tokio` dependency now declares the `sync`, `rt`, and `macros` features the
  server uses. Without them a consumer that depended on the library alone could
  not compile it; every local test command masked the omission, because
  `[dev-dependencies]` enabled those features and Cargo unifies them across
  targets.

### Security

- No `unsafe` code anywhere in the crate, enforced by `forbid(unsafe_code)`
  (NF-R-011).
- Malformed, truncated, oversized, or hostile peer input produces a typed error
  and never a panic, an out-of-bounds access, or an unbounded allocation
  (NF-R-012), pinned by a property-based suite over generated byte sequences and
  every truncation prefix of a valid ADU (NF-R-014).

[Unreleased]: https://github.com/TumbleOwlee/rust-modbus/commits/main
