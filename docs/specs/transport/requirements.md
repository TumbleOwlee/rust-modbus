# Transport — Requirements

Normative behavior of the transport area: TCP sockets and RTU serial ports, the rules that determine where one ADU ends and the next begins, connection setup and teardown, and read/write timeout semantics at the byte level.

This area is **role-agnostic**: it is used identically by the client and the server. What a byte sequence *means* belongs to [`../frame/`](../frame/); what a role does about it belongs to [`../client/`](../client/) or [`../server/`](../server/).

IDs are stable and append-only (`TR-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (transport types and configuration fields), [`edge-cases.md`](./edge-cases.md) (boundary and error behavior, stated limitations).

---

## Transport

**TR-R-001** — The transport layer operates over any type implementing `AsyncRead + AsyncWrite + Unpin + Send`. It requires no concrete socket or serial port type.

**TR-R-002** — The crate provides `FrameTransport<S, F>`, generic over such a stream `S` and a framing `F: Framing`, exposing asynchronous methods to send and receive requests and responses. It is role-agnostic: a client and a server use the same type, differing only in which direction they send and which they receive.

**TR-R-003** — Sending encodes the ADU through `F` and writes every byte of it before returning successfully. A partial write is never reported as success.

**TR-R-004** — Receiving yields exactly one ADU per call.

**TR-R-082** — Bytes read beyond an ADU's boundary are retained for the following receive call (TR-R-004) and never discarded.

**TR-R-005** — A frame that fails to decode does not desynchronize the stream: the transport consumes exactly that frame's bytes, surfaces the error, and remains usable for the next call.

**TR-R-044** — A receive that fails **before** an ADU has been delimited, on a self-locating framing (FR-R-144), discards the bytes accumulated for that attempt, so the next receive begins at the next boundary the wire provides. This complements TR-R-005, which governs a frame that was delimited and then failed to decode.

**TR-R-083** — A receive that fails **before** an ADU has been delimited, on a framing that is not self-locating (FR-R-144), retains the accumulated bytes, and the failure is terminal for that stream (contrast TR-R-044).

**TR-R-043** — A transport encodes each outgoing ADU into a single buffer that it owns and reuses across frames, clearing its contents but retaining its capacity between sends, so that sending in steady state performs no allocation.

**TR-R-084** — The length of the outgoing buffer of TR-R-043 never exceeds the framing's maximum ADU length.

**TR-R-075** — The crate provides a `ClientTransport<F>` trait, exposing `send_request`/`recv_response` for a client-side exchange over framing `F`. It is implemented by `FrameTransport<S, F>` for any `S: AsyncRead + AsyncWrite + Unpin + Send` and by `UdpTransport<F>` (TR-R-070), so a client-side consumer can be written generically over either transport.

**TR-R-081** — The futures returned by `ClientTransport::send_request` and `ClientTransport::recv_response` are `Send`, so that a future awaiting `Client::call` inside a function generic over `T: ClientTransport<F>` and `F: Framing` can be handed to a multi-threaded spawner (e.g. `tokio::spawn`) without naming a concrete transport or framing.

---

## Framing boundaries

**TR-R-010** — Over TCP, an ADU's boundary is determined by the MBAP length field: six bytes are read, the length field validated per FR-R-105, and exactly `6 + length` bytes constitute the ADU.

**TR-R-085** — Over TCP, the MBAP length field (TR-R-010) never sizes a read or an allocation before it is validated.

**TR-R-011** — Over RTU, an ADU's boundary is determined by inter-frame silence of at least 3.5 character times, a character time being computed from the configured frame format as `(1 + data bits + parity bits + stop bits) / baud rate`.

**TR-R-086** — Above 19200 baud, the RTU inter-frame interval of TR-R-011 is fixed at 1.75 ms.

**TR-R-012** — Over ASCII, an ADU begins at a `:` and ends at the first CR LF following it.

**TR-R-087** — Over ASCII, bytes preceding a `:` (TR-R-012) are discarded without error, however many there are, and are never held in memory beyond the framing's maximum ADU length.

**TR-R-013** — Receiving never buffers more than the framing's `MAX_ADU_LEN` bytes for a single ADU. Input exceeding it fails with the oversized-ADU error rather than growing the buffer.

**TR-R-045** — Over RTU-over-stream framing, an ADU's boundary is determined by applying the derivation of FR-R-146 to the bytes buffered so far, reading further bytes only when it reports that more are needed, and consuming exactly the extent it yields. No inter-frame timing is consulted.

**TR-R-046** — A receive on a framing whose boundary is derived from content retains the bytes it gathered when the derivation fails, on the same terms TR-R-083 sets for a length-prefixed framing: the failure is terminal for that stream, and discarding would only conceal it.

**TR-R-014** — A stream that ends cleanly between two ADUs reports end-of-stream.

**TR-R-088** — A stream that ends part-way through an ADU reports a connection-closed error, distinct from the end-of-stream of TR-R-014.

**TR-R-077** — A read-only frame reader is available for a stream that offers only `AsyncRead` (no `AsyncWrite`), for a listen-only serial port or bytes replayed from a capture. It applies the same per-framing boundary rule as `FrameTransport` (TR-R-010, TR-R-011, TR-R-012, TR-R-045) and never requires a write half.

**TR-R-078** — The read-only frame reader takes a direction (FR-R-005) at construction, consulted only by a boundary derivation that needs it (RTU-over-stream, FR-R-146/TR-R-045); a boundary that does not depend on direction (TCP, RTU, ASCII) ignores it.

**TR-R-079** — The read-only frame reader yields the raw bytes of one complete ADU per call, undecoded — header/PDU split and request-vs-response decoding remain the caller's job via the existing `Framing::decode_request`/`decode_response`. It performs no direction guessing of its own.

**TR-R-080** — After a boundary failure, the read-only frame reader's behavior matches `FrameTransport`'s: a self-locating boundary (FR-R-144) recovers on the next call; a non-self-locating boundary (RTU-over-stream) is terminal for that reader, on the same terms as TR-R-046.

---

## TCP

**TR-R-020** — The crate provides a TCP connector taking a socket address and a configuration, returning a `FrameTransport` over the connected socket.

**TR-R-021** — Connecting observes a connect timeout, defaulting to 5 seconds.

**TR-R-089** — Expiry of the connect timeout (TR-R-021) surfaces as the timeout error, distinct from a refused connection.

**TR-R-022** — `TCP_NODELAY` is enabled by default, and the default is overridable. Modbus is request/response, so Nagle delay is latency with no benefit.

**TR-R-023** — The crate provides a TCP listener that binds an address and accepts connections, yielding a `FrameTransport` per accepted connection.

**TR-R-024** — The TCP connector (TR-R-020) and the TCP listener (TR-R-023) are each usable with any framing, not with Modbus TCP framing alone, so that a socket carrying RTU-over-stream ADUs is established, configured, and accepted by the same code paths and with the same configuration (TR-R-021, TR-R-022) as one carrying MBAP-framed ADUs.

---

## RTU serial

**TR-R-030** — The crate provides a serial opener taking a device path and a configuration, returning a `FrameTransport` over the opened port.

**TR-R-031** — Serial configuration defaults to the Modbus serial-line defaults: 19200 baud, 8 data bits, even parity, 1 stop bit, no flow control. Each field is independently settable.

**TR-R-032** — Serial support is gated behind an off-by-default `rtu` feature, so a TCP-only consumer does not acquire a serial dependency.

**TR-R-033** — RTU-over-stream framing is available with the `rtu` feature off. It opens no serial port and derives no character time, so gating it behind the serial backend would deny a TCP-only consumer a purely TCP capability (TR-R-032).

**TR-R-034** — Every type appearing in the signature of a public serial item is nameable through this crate. In particular the serial stream type underlying `SerialTransport`, `RtuClient` and `AsciiClient` is re-exported under the `rtu` feature, so a consumer can name it without declaring the serial backend as a dependency of its own.

---

## Errors

**TR-R-040** — I/O failures surface as a typed error carrying the underlying `std::io::ErrorKind`.

**TR-R-041** — Timeouts surface as a distinct timeout error naming what timed out.

**TR-R-090** — A transport that has timed out mid-ADU is treated as desynchronized and is not usable for a further receive (TR-R-041).

**TR-R-042** — The transport area imposes no response timeout. Per-request timing is the client's (`CL-R-*`); the only timeouts here are connect, RTU inter-frame silence, and ASCII's inter-character timeout (TR-R-076).

**TR-R-048** — The inter-frame interval of TR-R-011 has no effect on RTU-over-stream framing, and no idle-gap heuristic is offered as a boundary rule over a socket. A gap in a TCP stream measures the network and the peer's buffering, not the bus: it would split a frame the network fragmented and join two the gateway coalesced.

**TR-R-076** — Over ASCII, once the start byte has been seen, a receive that produces no further byte within a configurable inter-character timeout is abandoned: the accumulated bytes are discarded per TR-R-044 (ASCII is self-locating, FR-R-144) and surfaced as a distinct timeout error.

**TR-R-091** — The ASCII inter-character timeout of TR-R-076 defaults to 1 second, matching the Modbus specification, and is not derived from baud rate as TR-R-011's RTU interval is.

---

## RS-485 kernel direction control

**TR-R-050** — The crate, on Linux and only under the off-by-default `rs485` feature, supports configuring the kernel's RS-485 direction-control mode (`TIOCSRS485`) for a serial port opened by TR-R-030: whether it is enabled, the RTS polarity asserted while transmitting, and the delay before and after a transmission during which RTS is held. No application-driven GPIO hook is provided; direction control is delegated entirely to the kernel driver.

**TR-R-051** — The `rs485` feature is off by default and implies the `rtu` feature, so RS-485 configuration is only reachable through the serial opener it configures.

**TR-R-052** — `SerialConfig` carries an `rs485: Option<Rs485Config>` field, present only when the `rs485` feature is enabled, so a build without the feature has no field whose value could be silently ignored.

**TR-R-092** — The `rs485` field of TR-R-052 defaults to `None`, which requests no RS-485 configuration and issues no ioctl.

**TR-R-053** — `open_serial`, when the opened configuration's `rs485` field is `Some`, issues the `TIOCSRS485` ioctl with the requested flags and delays after the port is opened and before the transport is returned to the caller, so a caller never holds a transport whose direction control silently failed to apply.

**TR-R-054** — On a target whose `target_os` is not `linux`, or when the opened driver's `TIOCSRS485` ioctl fails with an error indicating the mode is not implemented, `open_serial` fails with a typed error distinguishing "RS-485 not supported" from an ordinary I/O failure, and the port is not returned to the caller.

**TR-R-055** — The `TIOCSRS485` ioctl call is the crate's only unsafe code, compiled only when the `rs485` feature is enabled and only for `target_os = "linux"`; every other build configuration compiles with zero unsafe code, per NF-R-011.

**TR-R-056** — `Rs485Config`'s pre- and post-send delays are expressed as `Duration` and truncated to whole milliseconds at the point they are written to the ioctl, since the kernel field's own resolution is one millisecond.

**TR-R-093** — An `Rs485Config` delay (TR-R-056) whose millisecond count does not fit in a `u32` fails with `Error::Configuration` rather than wrapping.

**TR-R-057** — The RTS level asserted after a transmission is the logical complement of the level asserted during it, matching the drive-enable/idle-disable pattern every two-wire RS-485 transceiver expects; the crate exposes no independently configurable after-send polarity.

**TR-R-058** — Behind the crate's `serde` feature, `SerialConfig`, `TcpConfig`, `TransportConfig`, `DataBits`, `Parity`, `StopBits` and `FlowControl` implement `serde::Serialize` and `serde::Deserialize` with no validation beyond their field and variant types'.

**TR-R-094** — Behind the crate's `serde` feature, every `Duration` field of a TR-R-058 type keeps `Duration`'s own serde representation rather than a count in any single unit, so that every value `Duration` can hold survives a round trip exactly — both `TransportConfig`'s baud-derived `inter_frame_interval`, whose default is 2,005,208 ns, and a duration whose nanosecond count would not fit an integer field.

**TR-R-095** — Behind the crate's `serde` feature, a deserialized `SerialConfig` (TR-R-058) with a zero baud rate is accepted exactly as direct construction accepts it: the existing configuration error fires the first time the value is used, not at deserialize time.

**TR-R-059** — Behind the crate's `serde` feature together with the `rs485` feature, `Rs485Config` and `RtsPolarity` implement `serde::Serialize` and `serde::Deserialize` on the same terms as TR-R-058 and TR-R-094, both delays included. The truncation TR-R-056 applies belongs to the ioctl boundary, not to the configuration: a delay survives a round trip exactly as it was configured, and only the kernel sees whole milliseconds.

---

## TLS

**TR-R-060** — TLS transport is available over TCP only, gated behind an off-by-default `tls` feature; `tls` implies `std` and is absent from a `no_std` build.

**TR-R-061** — TLS is implemented with `rustls` via `tokio-rustls`; the crate depends on no other TLS implementation.

**TR-R-062** — The crate provides a TLS connector taking a socket address, `TcpConfig`, and `TlsClientConfig`, performing a TCP connect then a TLS handshake, returning a `FrameTransport` over the resulting stream.

**TR-R-096** — A TLS connector (TR-R-062) handshake failure surfaces as an error distinct from a TCP connect failure.

**TR-R-063** — The crate provides a TLS listener wrapping a bound TCP listener, performing the TLS handshake per accepted connection before yielding a `FrameTransport`, with the same per-connection independence as the plain listener (SV-R-030).

**TR-R-064** — The handshake occurs entirely inside the TLS connector/listener, before `FrameTransport` construction; `FrameTransport` and the plain-TCP connector/listener require no change, since `tokio_rustls::TlsStream` already satisfies TR-R-001's bound.

**TR-R-065** — `TlsClientConfig` carries a `ServerCertVerification` policy — `Verify(RootStore)` (defaultable to platform-native roots) or the explicitly-named `DangerousDisableVerification` — plus an optional client cert/key for client auth. No boolean/`Option` spelling reaches "skip verification" silently.

**TR-R-066** — `TlsServerConfig` carries the server's cert/key and a `ClientCertPolicy`: `Require(RootStore)` (validates the client cert chain against `RootStore`), `AllowAny` (requires a client cert be presented but performs no chain/identity validation, mirroring TR-R-065's `DangerousDisableVerification`), or `None` (encryption-only, no client cert requested). No boolean/`Option` spelling reaches "skip verification" silently — only the explicitly-named `AllowAny` variant may do so.

**TR-R-067** — TLS handshake failure surfaces as a distinct `Error::TlsHandshake { source: rustls::Error, peer_cert: Option<CertificateDer<'static>> }` variant, separate from `Io` and `Timeout`. `source` carries the underlying `rustls` error. Because `rustls::Error` has no `Eq`, `Error` derives `Eq` only when the `tls` feature is disabled; `PartialEq`/`Clone`/`Debug` hold unconditionally.

**TR-R-068** — The crate exports `MODBUS_TLS_PORT: u16 = 802` (documentation constant only); no API applies it implicitly — `connect_tls`/the TLS listener each take an explicit `SocketAddr`, same as their plain-TCP counterparts.

**TR-R-069** — On a server-side handshake rejecting a client certificate under `ClientCertPolicy::Require`, `Error::TlsHandshake.peer_cert` is `Some` with the offered certificate. `peer_cert` is `None` when no client cert was offered, when the failure has another cause, or on any client-side (`connect_tls`) handshake failure — capturing the rejected server cert there is out of scope.

---

## UDP

**TR-R-070** — The crate provides `UdpTransport`, a framing-generic transport over a UDP socket bound to one fixed peer address for its lifetime, sending and receiving exactly one ADU per datagram. Unlike `FrameTransport` (TR-R-002), it performs no partial-frame accumulation: each datagram is handed whole to `F::decode_request`/`F::decode_response` on receive and built whole by `F::encode_request_into`/`F::encode_response_into` before one `send` on transmit, since the OS already delimits the datagram boundary.

**TR-R-071** — The crate provides `connect_udp`, taking a peer socket address and a `UdpConfig`, returning a `UdpTransport` bound to that peer. Associating a UDP socket with a peer performs no network handshake; TR-R-021's connect timeout does not apply to it.

**TR-R-072** — The crate provides a UDP server entry point taking an already-bound UDP socket, usable with any framing, mirroring TR-R-024's framing-agnostic stance.

**TR-R-073** — Sending on `UdpTransport` encodes the ADU into the TR-R-043 reused buffer and passes it to one `send` call.

**TR-R-097** — Sending on `UdpTransport` (TR-R-073) refuses an encoded ADU exceeding the OS's maximum UDP payload or the framing's `MAX_ADU_LEN` before any I/O is attempted.

**TR-R-074** — Receiving on `UdpTransport` yields exactly one ADU per datagram.

**TR-R-098** — On `UdpTransport` (TR-R-074), a datagram that fails to decode surfaces as a typed error without affecting any later receive — a datagram transport has no stream state to desynchronize, so TR-R-005, TR-R-044 and TR-R-083 do not apply to it.
