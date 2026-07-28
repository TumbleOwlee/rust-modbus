# PRD — rust-modbus

Product framing for `rust-modbus`. Normative behavior lives in
[`docs/specs/`](./docs/specs/); structure lives in
[`ARCHITECTURE.md`](./ARCHITECTURE.md). This document states *why* the library
exists and what it is and is not for — it does not restate requirements.

## Overview

`rust-modbus` is a Rust library providing **asynchronous Modbus client and
server** implementations over both **Modbus TCP** and **Modbus RTU**. It is a
library only: it ships no binary, no CLI, and no UI. Consumers embed it to talk
to Modbus devices, or to expose their own state as a Modbus slave.

The four combinations are all first-class and none is an afterthought:

| | TCP | RTU |
|---|---|---|
| **Client** (initiator) | ✅ | ✅ |
| **Server** (responder) | ✅ | ✅ |

## Goals

- **Correctness against the Modbus standard first.** Wire encoding, exception
  responses, CRC, and MBAP framing follow the published specification, and are
  tested against byte vectors derived from it rather than from our own output.
- **Async-first on Tokio.** Non-blocking client and server, usable from an
  existing async application without a thread pool or blocking bridge.
- **Symmetric client and server.** The same frame layer serves both roles, so a
  fix in encoding benefits both and the two cannot drift.
- **Robust against hostile input.** Truncated, malformed, or oversized frames
  produce typed errors — never a panic, an index-out-of-bounds, or an unbounded
  allocation.
- **Typed, ergonomic public API.** Register addresses, function codes, and
  failure modes are types, not integers and strings.
- **Testable by construction.** Transports sit behind a seam so client and server
  logic is exercised without hardware or fixed ports. Coverage floor of 80%.
- **ASCII framing for testability.** Modbus ASCII is encodable and decodable at
  the frame layer, so frames are readable in test fixtures and comparable against
  upstream tooling by eye.

## Non-goals

- **No TUI, CLI, or GUI.** Presentation belongs to consumers.
- **No blocking/sync API** in the initial scope. Async-first; a blocking facade
  would be a separate product decision.
- **No ASCII *transport*.** ASCII framing exists at the frame layer for test
  fixtures and interoperability checking; operating a serial port in ASCII mode
  is not in scope unless later specified.
- **No device-specific quirk layer.** Vendor deviations from the standard are the
  consumer's problem unless a requirement says otherwise.
- **No persistence.** The server's data store is in-memory; durability is the
  consumer's concern.
- **No transport beyond TCP and RTU serial** (no UDP, no RTU-over-TCP gateway
  emulation) unless later specified.

## Users

- **Rust application developers** integrating with industrial equipment (PLCs,
  inverters, meters, BMS) as a Modbus master.
- **Rust application developers** exposing their own service as a Modbus slave to
  existing SCADA/HMI systems.
- **Test and simulation tooling** needing a programmable Modbus endpoint on both
  sides of the wire.

## Success criteria

- Both roles interoperate with at least one independent, widely used Modbus
  implementation over both transports.
- A malformed-input fuzz/truncation suite produces errors and never a panic.
- Line coverage stays at or above 80%, enforced in CI.
- Every requirement in `docs/specs/` is pinned by a test citing its ID, except
  those explicitly listed as intentionally untested.
