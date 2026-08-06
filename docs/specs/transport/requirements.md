# Transport — Requirements

Normative behavior of the transport area: TCP sockets and RTU serial ports, the
rules that determine where one ADU ends and the next begins, connection setup and
teardown, and read/write timeout semantics at the byte level.

This area is **role-agnostic**: it is used identically by the client and the
server. What a byte sequence *means* belongs to [`../frame/`](../frame/); what a
role does about it belongs to [`../client/`](../client/) or
[`../server/`](../server/).

IDs are stable and append-only (`TR-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (transport types and
configuration fields), [`edge-cases.md`](./edge-cases.md) (boundary and error
behavior, stated limitations).

---

## 1. Transport

**TR-R-001** — The transport layer shall operate over any type implementing
`AsyncRead + AsyncWrite + Unpin + Send`. It shall not require a concrete socket
or serial port type.

**TR-R-002** — The crate shall provide `FrameTransport<S, F>`, generic over such
a stream `S` and a framing `F: Framing`, exposing asynchronous methods to send
and receive requests and responses. It shall be role-agnostic: a client and a
server use the same type, differing only in which direction they send and which
they receive.

**TR-R-003** — Sending shall encode the ADU through `F` and write every byte of
it before returning successfully. A partial write shall not be reported as
success.

**TR-R-004** — Receiving shall yield exactly one ADU per call. Bytes read beyond
the ADU's boundary shall be retained for the following call and shall not be
discarded.

**TR-R-005** — A frame that fails to decode shall not desynchronize the stream:
the transport shall consume exactly that frame's bytes, surface the error, and
remain usable for the next call.

**TR-R-044** — A receive that fails **before** an ADU has been delimited shall, on a
self-locating framing (FR-R-144), discard the bytes accumulated for that attempt, so
the next receive begins at the next boundary the wire provides. On a framing that is
not self-locating, the accumulated bytes shall be retained and the failure is terminal
for that stream. This complements TR-R-005, which governs a frame that was delimited
and then failed to decode.

**TR-R-043** — A transport shall encode each outgoing ADU into a single buffer
that it owns and reuses across frames, clearing its contents but retaining its
capacity between sends, so that sending in steady state performs no allocation.
That buffer shall never exceed the framing's maximum ADU length.

---

## 2. Framing boundaries

**TR-R-010** — Over TCP, an ADU's boundary shall be determined by the MBAP length
field: six bytes are read, the length field validated per FR-R-105, and exactly
`6 + length` bytes constitute the ADU. The length shall never size a read or an
allocation before it is validated.

**TR-R-011** — Over RTU, an ADU's boundary shall be determined by inter-frame
silence of at least 3.5 character times. A character time shall be computed from
the configured frame format as `(1 + data bits + parity bits + stop bits) / baud
rate`. Above 19200 baud the interval shall be fixed at 1.75 ms.

**TR-R-012** — Over ASCII, an ADU shall begin at a `:` and end at the first CR LF
following it. Bytes preceding a `:` shall be discarded, and the discard shall be
bounded by the framing's maximum ADU length.

**TR-R-013** — Receiving shall never buffer more than the framing's
`MAX_ADU_LEN` bytes for a single ADU. Input exceeding it shall fail with the
oversized-ADU error rather than grow the buffer.

**TR-R-045** — Over RTU-over-stream framing, an ADU's boundary shall be
determined by applying the derivation of FR-R-146 to the bytes buffered so far,
reading further bytes only when it reports that more are needed, and consuming
exactly the extent it yields. No inter-frame timing shall be consulted.

**TR-R-046** — A receive on a framing whose boundary is derived from content
shall retain the bytes it gathered when the derivation fails, on the same terms
TR-R-044 sets for a length-prefixed framing: the failure is terminal for that
stream, and discarding would only conceal it.

**TR-R-014** — A stream that ends cleanly between two ADUs shall report
end-of-stream; one that ends part-way through an ADU shall report a distinct
connection-closed error.

---

## 3. TCP

**TR-R-020** — The crate shall provide a TCP connector taking a socket address
and a configuration, returning a `FrameTransport` over the connected socket.

**TR-R-021** — Connecting shall observe a connect timeout, defaulting to 5
seconds; expiry shall surface as the timeout error, distinct from a refused
connection.

**TR-R-022** — `TCP_NODELAY` shall be enabled by default. Modbus is
request/response, so Nagle delay is latency with no benefit; the default shall be
overridable.

**TR-R-023** — The crate shall provide a TCP listener that binds an address and
accepts connections, yielding a `FrameTransport` per accepted connection.

**TR-R-024** — The TCP connector (TR-R-020) and the TCP listener (TR-R-023) shall
each be usable with any framing, not with Modbus TCP framing alone, so that a
socket carrying RTU-over-stream ADUs is established, configured, and accepted by
the same code paths and with the same configuration (TR-R-021, TR-R-022) as one
carrying MBAP-framed ADUs.

---

## 4. RTU serial

**TR-R-030** — The crate shall provide a serial opener taking a device path and a
configuration, returning a `FrameTransport` over the opened port.

**TR-R-031** — Serial configuration shall default to the Modbus serial-line
defaults: 19200 baud, 8 data bits, even parity, 1 stop bit, no flow control. Each
field shall be independently settable.

**TR-R-032** — Serial support shall be gated behind an off-by-default `rtu`
feature, so a TCP-only consumer does not acquire a serial dependency.

**TR-R-033** — RTU-over-stream framing shall be available with the `rtu` feature
off. It opens no serial port and derives no character time, so gating it behind
the serial backend would deny a TCP-only consumer a purely TCP capability
(TR-R-032).

**TR-R-034** — Every type appearing in the signature of a public serial item shall
be nameable through this crate. In particular the serial stream type underlying
`SerialTransport`, `RtuClient` and `AsciiClient` shall be re-exported under the
`rtu` feature, so a consumer can name it without declaring the serial backend as a
dependency of its own.

---

## 5. Errors

**TR-R-040** — I/O failures shall surface as a typed error carrying the
underlying `std::io::ErrorKind`.

**TR-R-041** — Timeouts shall surface as a distinct timeout error naming what
timed out. A transport that has timed out mid-ADU shall be treated as
desynchronized and shall not be reused for a further receive.

**TR-R-042** — The transport area shall not impose a response timeout.
Per-request timing is the client's (`CL-R-*`); the only timeouts here are connect
and RTU inter-frame silence.

**TR-R-048** — The inter-frame interval of TR-R-011 shall have no effect on
RTU-over-stream framing, and no idle-gap heuristic shall be offered as a boundary
rule over a socket. A gap in a TCP stream measures the network and the peer's
buffering, not the bus: it would split a frame the network fragmented and join
two the gateway coalesced.

---

## 6. RS-485 kernel direction control

**TR-R-050** — The crate shall, on Linux and only under the off-by-default `rs485`
feature, support configuring the kernel's RS-485 direction-control mode (`TIOCSRS485`)
for a serial port opened by TR-R-030: whether it is enabled, the RTS polarity asserted
while transmitting, and the delay before and after a transmission during which RTS is
held. No application-driven GPIO hook shall be provided; direction control is delegated
entirely to the kernel driver.

**TR-R-051** — The `rs485` feature shall be off by default and shall imply the `rtu`
feature, so RS-485 configuration is only reachable through the serial opener it configures.

**TR-R-052** — `SerialConfig` shall carry an `rs485: Option<Rs485Config>` field, present
only when the `rs485` feature is enabled, so a build without the feature has no field whose
value could be silently ignored. `None`, the default, requests no RS-485 configuration and
issues no ioctl.

**TR-R-053** — `open_serial`, when the opened configuration's `rs485` field is `Some`,
shall issue the `TIOCSRS485` ioctl with the requested flags and delays after the port is
opened and before the transport is returned to the caller, so a caller never holds a
transport whose direction control silently failed to apply.

**TR-R-054** — On a target whose `target_os` is not `linux`, or when the opened driver's
`TIOCSRS485` ioctl fails with an error indicating the mode is not implemented,
`open_serial` shall fail with a typed error distinguishing "RS-485 not supported" from an
ordinary I/O failure, and the port shall not be returned to the caller.

**TR-R-055** — The `TIOCSRS485` ioctl call shall be the crate's only unsafe code, compiled
only when the `rs485` feature is enabled and only for `target_os = "linux"`; every other
build configuration shall compile with zero unsafe code, per NF-R-011.

**TR-R-056** — `Rs485Config`'s pre- and post-send delays shall be expressed as `Duration`
and truncated to whole milliseconds at the point they are written to the ioctl, since the
kernel field's own resolution is one millisecond. A `Duration` whose millisecond count does
not fit in a `u32` shall fail with `Error::Configuration` rather than wrap.

**TR-R-057** — The RTS level asserted after a transmission shall be the logical complement
of the level asserted during it, matching the drive-enable/idle-disable pattern every
two-wire RS-485 transceiver expects; the crate shall not expose an independently
configurable after-send polarity.

**TR-R-058** — Behind the crate's `serde` feature, `SerialConfig`, `TcpConfig`,
`TransportConfig`, `DataBits`, `Parity`, `StopBits` and `FlowControl` shall implement
`serde::Serialize` and `serde::Deserialize` with no validation beyond their field and variant
types'. Every `Duration` field shall keep `Duration`'s own serde representation rather than a
count in any single unit, so that every value `Duration` can hold survives a round trip exactly
— both `TransportConfig`'s baud-derived `inter_frame_interval`, whose default is 2,005,208 ns,
and a duration whose nanosecond count would not fit an integer field. A deserialized
`SerialConfig` with a zero baud rate shall be
accepted exactly as direct construction accepts it: the existing configuration error fires the
first time the value is used, not at deserialize time.

**TR-R-059** — Behind the crate's `serde` feature together with the `rs485` feature,
`Rs485Config` and `RtsPolarity` shall implement `serde::Serialize` and `serde::Deserialize` on
the same terms as TR-R-058, both delays included. The truncation TR-R-056 applies belongs to
the ioctl boundary, not to the configuration: a delay survives a round trip exactly as it was
configured, and only the kernel sees whole milliseconds.

---

## 7. TLS

**TR-R-060** — TLS transport shall be available over TCP only, gated behind an off-by-default
`tls` feature; `tls` shall imply `std` and be absent from a `no_std` build.

**TR-R-061** — TLS shall be implemented with `rustls` via `tokio-rustls`; the crate shall
depend on no other TLS implementation.

**TR-R-062** — The crate shall provide a TLS connector taking a socket address, `TcpConfig`,
and `TlsClientConfig`, performing a TCP connect then a TLS handshake, returning a
`FrameTransport` over the resulting stream. Handshake failure shall surface as an error
distinct from a TCP connect failure.

**TR-R-063** — The crate shall provide a TLS listener wrapping a bound TCP listener,
performing the TLS handshake per accepted connection before yielding a `FrameTransport`, with
the same per-connection independence as the plain listener (SV-R-030).

**TR-R-064** — The handshake shall occur entirely inside the TLS connector/listener, before
`FrameTransport` construction; `FrameTransport` and the plain-TCP connector/listener require no
change, since `tokio_rustls::TlsStream` already satisfies TR-R-001's bound.

**TR-R-065** — `TlsClientConfig` shall carry a `ServerCertVerification` policy —
`Verify(RootStore)` (defaultable to platform-native roots) or the explicitly-named
`DangerousDisableVerification` — plus an optional client cert/key for client auth. No
boolean/`Option` spelling shall reach "skip verification" silently.

**TR-R-066** — `TlsServerConfig` shall carry the server's cert/key and a `ClientCertPolicy`:
`Require(RootStore)` or `None` (encryption-only, no client cert requested). No policy shall
accept an unverified client cert as authenticated.

**TR-R-067** — TLS handshake failure shall surface as a distinct `Error::TlsHandshake`
variant, separate from `Io` and `Timeout`.

**TR-R-068** — The crate shall export `MODBUS_TLS_PORT: u16 = 802` (documentation constant
only); no API applies it implicitly — `connect_tls`/the TLS listener each take an explicit
`SocketAddr`, same as their plain-TCP counterparts.
