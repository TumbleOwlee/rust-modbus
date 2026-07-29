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
| `client` | Async initiator: issues requests, matches responses, applies timeouts, handles retry and reconnect. Built on `frame` + `transport`. | [`client/`](./docs/specs/client/) |
| `server` | Async responder: accepts connections, dispatches decoded requests against the data store, generates exception responses. Built on `frame` + `transport`. | [`server/`](./docs/specs/server/) |
| `error` | The crate's single public error enum. Every fallible path in every module surfaces through it. | cross-cutting |

`frame` and `transport` have landed; `client` and `server` are still to be
specified. Inside `transport`: `mod.rs` holds `FrameTransport` and the boundary
readers, `tcp.rs` the connector and listener, `serial.rs` the port parameters and
the inter-frame timing they imply, and `rtu.rs` the port opener behind the `rtu`
feature.

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
   consumer state ◄─► │    server    │ ──┘
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

Async on **Tokio**. The client is driven by the caller's task; the server owns a
listener task and spawns a task per connection. Shared state (the server's data
store) is behind an async-safe lock; no protocol logic blocks the runtime.

*(Fill in the locking discipline, backpressure, and shutdown semantics as those
requirements land in `docs/specs/server/` and `docs/specs/transport/`.)*

## Testing structure

- Unit tests: `#[cfg(test)] mod tests` at the bottom of the module under test,
  functions prefixed `ut_`.
- Integration tests: `tests/*.rs`, functions prefixed `it_`.
- TCP tests bind port 0 and read the assigned port back — never a fixed port.
- RTU tests use a virtual/in-memory duplex pair; tests needing real hardware are
  ignored by default and never run in CI.
