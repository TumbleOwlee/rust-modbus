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
- **Reachability through transparent gateways.** A device behind a converter that
  forwards bare RTU ADUs over a socket is addressable with the same client and
  server types as any other, with the mode's limits stated in the specification
  rather than discovered on a live bus.

## Non-goals

- **No TUI, CLI, or GUI.** Presentation belongs to consumers.
- **No blocking *server*.** Async-first remains the primary surface, but a
  blocking *client* ships behind the off-by-default `sync` feature: it owns its
  runtime and delegates to the async client, so the two cannot diverge. A
  blocking server is out of scope — the thread structure serving inbound
  connections is the consumer's choice.
- **No ASCII *transport*.** ASCII framing exists at the frame layer for test
  fixtures and interoperability checking; operating a serial port in ASCII mode
  is not in scope unless later specified.
- **No device-specific quirk layer.** Vendor deviations from the standard are the
  consumer's problem unless a requirement says otherwise.
- **No data model at all.** The server declares a `Service` trait and ships no
  register tables (SV-R-005), so both storage and durability are the consumer's
  concern. What a coil or a register *means* is application state wearing a
  Modbus address, not protocol.
- **No interpretation of register contents.** No floating-point or 32/64-bit
  conversions spanning registers, no word-order or endianness options above the
  register, no scaling factors, no unit conversions, no enumerated status words.
  Modbus defines nothing wider than a 16-bit register: a device that spreads a
  float across two has made a private convention, which is why four incompatible
  word orders exist in the field where a standard would have left one. Combining
  registers is therefore application knowledge that belongs with the device's
  register map, and `Vec<RegisterValue>` is the honest handoff point. The
  boundary is where the standard's own authority ends — byte order *within* a
  register is protocol and is implemented (FR-R-003, big-endian on the wire);
  order *across* registers is convention and stays the caller's.
- **No transport beyond TCP, RTU serial, and RTU framing over TCP** (no UDP)
  unless later specified. RTU-over-TCP is in scope because the installed base of
  serial-to-Ethernet converters is large and a user who has bought one cannot
  choose what it speaks; it is supported on the honest terms its wire allows —
  a content-derived frame boundary, function codes whose length is not derivable
  refused rather than guessed, and a link that does not survive a bad frame.

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

## Capability areas

The specification is split by area; each owns its behavior end to end.

| Area | Covers | ID prefix |
|---|---|---|
| [`frame/`](./docs/specs/frame/) | PDU structure, function code taxonomy, exception responses, robustness, buffer reuse, serde/Display | `FR-R-nnn` |
| [`frame-data-access/`](./docs/specs/frame-data-access/) | Bit/register access, file record access, serial-line diagnostics, MEI | `FR-R-nnn` / `FR-DA-R-nnn` |
| [`frame-adu/`](./docs/specs/frame-adu/) | RTU/TCP/ASCII ADU, RTU over byte stream, CRC-16, MBAP header, framing abstraction | `FR-R-nnn` / `FR-ADU-R-nnn` |
| [`client/`](./docs/specs/client/) | Async client API, request issuing, response matching, timeouts, retry and reconnect | `CL-R-nnn` |
| [`server/`](./docs/specs/server/) | Async server, connection handling, request dispatch, data store, exception generation | `SV-R-nnn` |
| [`transport/`](./docs/specs/transport/) | TCP sockets and RTU serial ports, framing boundaries, connection lifecycle | `TR-R-nnn` |

Cross-cutting concerns live in [`docs/specs/non-functional-requirements.md`](./docs/specs/non-functional-requirements.md).
