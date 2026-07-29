# Architecture

Structure of `rust-modbus`. Product framing: [`PRD.md`](./PRD.md). Normative
behavior: [`docs/specs/`](./docs/specs/).

This document describes *where things live*. It is not normative — when it
disagrees with `docs/specs/`, the spec wins and this file is the one to fix.

## Shape

A **single library crate**, `rust-modbus`, organised into modules. There is no
workspace and no binary target. The crate is split into modules rather than
crates because the pieces are small, share one error type, and version in
lockstep; if a piece later earns an independent release cadence, that is a
structural decision to raise, not to make silently.

## Module map

| Module | Responsibility | Spec area |
|---|---|---|
| `frame` | Modbus PDU and ADU encode/decode: function codes, request/response bodies, exception responses, CRC-16 (RTU), LRC + hex characters (ASCII), MBAP header (TCP). Pure, no I/O. | [`frame/`](./docs/specs/frame/) |
| `transport` | Byte-level transports: TCP sockets and RTU serial ports. Owns framing boundaries (where one ADU ends), connection setup and teardown. Role-agnostic. | [`transport/`](./docs/specs/transport/) |
| `client` | Async initiator: issues requests, matches responses, applies timeouts. No retry or reconnect (CL-R-033). Built on `frame` + `transport`. | [`client/`](./docs/specs/client/) |
| `server` | Async responder: serves a listener or a single link, dispatches decoded requests to the consumer's `Service`, turns a refusal into an exception response, and drains on shutdown. Owns **no** data store (SV-R-005). Built on `frame` + `transport`. | [`server/`](./docs/specs/server/) |
| `error` | The crate's single public error enum. Every fallible path in every module surfaces through it. | cross-cutting |

All four areas have landed. Inside `transport`: `mod.rs` holds `FrameTransport` and the boundary
readers, `tcp.rs` the connector and listener, `serial.rs` the port parameters and
the inter-frame timing they imply, and `rtu.rs` the port opener behind the `rtu`
feature.

Inside `client`: `mod.rs` holds `Client` and its request methods, `framing.rs`
the `ClientFraming` bridge. That bridge exists because the framings disagree on
exactly three points — how a header is built, when a reply answers a request, and
which unit identifier broadcasts — so one generic client covers all three
framings and the differences live in one file.

Inside `server`: `mod.rs` holds `Server`, the accept loop, and the one
per-connection exchange both entry points run; `service.rs` the `Service` trait
and the `Connection`/`Disconnect` values its notifications carry; `handle.rs` the
shutdown handle; `framing.rs` the `ServerFraming` bridge, which is the client
bridge's mirror — a responder reads a header instead of building one, so what it
needs is which unit a header addresses and whether that unit is the broadcast.

Domain values (`UnitId`, `Address`, `Quantity`, `RegisterValue`, …) are defined
in `frame/value.rs` and used by every layer above it. They are transparent
newtypes (FR-R-007): the wire is unchanged, but two fields of equal width and
unequal meaning cannot be swapped at a call site.

Where an ADU *ends* belongs to the framing, not to the socket: `Framing::boundary`
(FR-R-122) describes the rule — a length prefix, a delimiter pair, or silence —
and `transport` is what applies it to a stream. That keeps the frame area free of
I/O and leaves one place per rule.

## Data flow

```
                      ┌──────────────┐
   consumer code ───► │    client    │ ──┐
                      └──────────────┘   │
                                         ├─► frame (encode ADU) ─► transport ─► wire
                      ┌──────────────┐   │
   consumer Service ◄► │    server    │ ──┘
                      └──────────────┘
                                         ◄─ frame (decode ADU) ◄─ transport ◄─ wire
```

`frame` is the shared spine: both roles encode and decode through the same code,
so a wire-format fix cannot land on one side only. `frame` performs no I/O and is
therefore testable purely on byte vectors — that is deliberate, and the main
reason the coverage floor is achievable without hardware.

`transport` is the seam that makes client and server testable without a network
or a serial device: anything that behaves as an async duplex byte stream can be
substituted, so loopback and in-memory pairs stand in for real endpoints in tests.

## Concurrency model

Async on **Tokio**. The client is driven by the caller's task and permits one
request in flight, enforced by `&mut self` (CL-R-005). The server's accept loop
spawns a task per connection (SV-R-030) and every task shares one `Arc<Service>`;
within a connection, requests are answered one at a time — read, dispatch,
answer, read again.

The crate holds **no lock of its own**. Because `Service` takes `&self`, any
mutable state belongs to the consumer's type and so does its locking (SV-R-003,
`docs/specs/server/data-contract.md`). That is the deliberate consequence of
shipping no data store: there is no crate-owned state left to contend on, and no
lock discipline for a consumer to have to fit around.

Backpressure is the transports': a connection reads one ADU at a time and never
buffers more than one ADU's worth (TR-R-013), so a fast peer cannot make the
server allocate without bound.

Shutdown is a `tokio::sync::watch` flag plus its own drain. Every connection task
holds a receiver, so `ServerHandle::shutdown` sets the flag and then awaits
`Sender::closed()` — which completes exactly when the last connection task has
dropped its receiver (SV-R-044). No counter, and nothing to leak: a request
already dispatched finishes and answers first (SV-R-042).

## Testing structure

- Unit tests: `#[cfg(test)] mod tests` at the bottom of the module under test,
  functions prefixed `ut_`.
- Integration tests: `tests/*.rs`, functions prefixed `it_`.
- TCP tests bind port 0 and read the assigned port back — never a fixed port.
- RTU tests use a virtual/in-memory duplex pair; tests needing real hardware are
  ignored by default and never run in CI.
- `tests/server_tcp.rs` is the full-stack test: this crate's client against this
  crate's server. Concurrency is *asserted*, not hoped for — the unit test
  `ut_connections_are_served_concurrently` holds every request until three are in
  flight together, so a server that serialised them would fail rather than merely
  run slowly.
- `tests/interop_tcp.rs` checks the client against a foreign server; it is ignored
  by default because it needs one listening on `127.0.0.1:5020`.
