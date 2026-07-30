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

---

## 4. RTU serial

**TR-R-030** — The crate shall provide a serial opener taking a device path and a
configuration, returning a `FrameTransport` over the opened port.

**TR-R-031** — Serial configuration shall default to the Modbus serial-line
defaults: 19200 baud, 8 data bits, even parity, 1 stop bit, no flow control. Each
field shall be independently settable.

**TR-R-032** — Serial support shall be gated behind an off-by-default `rtu`
feature, so a TCP-only consumer does not acquire a serial dependency.

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
