# rust-modbus

Async Modbus **client** and **server** for Rust, over **Modbus TCP** and
**Modbus RTU** serial.

All four combinations are first-class, and none is an afterthought:

|                          | TCP | RTU serial |
| ------------------------ | :-: | :--------: |
| **Client** (initiator)   | ✅  |     ✅     |
| **Server** (responder)   | ✅  |     ✅     |

Both roles sit on one shared frame layer, so a fix in encoding benefits both and
the two cannot drift apart. Modbus **ASCII** framing is encodable and decodable
too, but only as a frame format — see [Deliberate omissions](#deliberate-omissions).

- Async-first on [Tokio](https://tokio.rs). No thread pool, no blocking bridge.
- **Typed, not stringly.** Addresses, quantities, register values and unit
  identifiers are distinct types that share a width but cannot be swapped at a
  call site. Every failure is a variant of one error enum.
- **Robust against hostile input.** Truncated, malformed or oversized frames
  produce a typed error — never a panic, an out-of-bounds slice, or an unbounded
  allocation. `#![forbid(unsafe_code)]`.
- **`no_std` + `alloc`** with default features off: the frame layer needs no
  operating system.
- **Testable without hardware.** Transports are a generic bound over any async
  duplex stream, so an in-memory pipe substitutes for a socket or a serial port.

## Install

```sh
cargo add rust-modbus
# for RTU serial ports:
cargo add rust-modbus --features rtu
```

### Feature flags

| Feature | Default | What it gates |
| --- | --- | --- |
| `std` | **on** | Everything above the frame layer: `Client`, `Server`, `FrameTransport`, TCP. Pulls in Tokio. |
| `rtu` | off | Opening a real serial port: `open_serial`, `SerialTransport`, `RtuClient`, `AsciiClient`. Implies `std`. |

`rtu` is off by default so a TCP-only consumer acquires no serial dependency. It
gates *only opening a port* — RTU and ASCII **framing** are always available, so
`Client<S, Rtu>` over any duplex stream (an in-memory pipe, a socket, a pty)
works with the feature off. That is how this crate tests RTU in CI.

Turning `std` off leaves a `no_std` + `alloc` crate that still encodes and
decodes every supported function code over every framing.

## Client quickstart

```rust,no_run
use std::time::Duration;

use rust_modbus::{
    Address, Client, ClientConfig, Quantity, RegisterValue, TcpConfig, UnitId, connect_tcp,
};

#[tokio::main]
async fn main() -> rust_modbus::Result<()> {
    // Connecting is separate from constructing the client: a `Client` is built
    // from a transport that is already established, so nothing reconnects behind
    // your back. The client also never retries — that policy stays yours.
    let address = "127.0.0.1:502".parse().expect("a literal socket address");
    let transport = connect_tcp(address, TcpConfig::default()).await?;
    let mut client = Client::with_config(
        transport,
        ClientConfig { response_timeout: Duration::from_secs(1) },
    );

    // Every request takes the unit identifier first: it addresses a device
    // *behind* the socket, which matters when the peer is a serial gateway.
    let registers = client
        .read_holding_registers(UnitId(1), Address(0), Quantity(4))
        .await?;
    println!("{registers:?}");

    client
        .write_single_register(UnitId(1), Address(0), RegisterValue(0x1234))
        .await?;

    Ok(())
}
```

One in-flight request at a time is enforced by `&mut self` on every request
method, not by a runtime flag — give each connection its own client to overlap
requests. A device *refusing* a request is not an I/O failure: it answers with a
Modbus exception, which surfaces as `Error::Exception` carrying the function code
and the exception code.

`Client::call` is the escape hatch: it hands back the response exactly as
received, exception responses and echoes included, and is how a function code
outside the named set is issued.

## Server quickstart

The crate ships **no data model** — no register tables, no built-in service. A
server is your own type answering requests, which is a deliberate choice, not a
gap ([SV-R-005](./docs/specs/server/data-contract.md)).

```rust,no_run
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_modbus::{
    Connection, ExceptionCode, RegisterValue, RequestPdu, ResponsePdu, Server, ServerConfig,
    Service, TcpListener, UnitId,
};

// Cheap to clone, and cloning *shares* — which is how you keep a view of your
// own state after handing a copy to `Server::new`. It has to be: the orphan rule
// forbids `impl Service for Arc<MyType>` outside this crate.
#[derive(Clone, Default)]
struct Plc {
    holding: Arc<Mutex<HashMap<u16, u16>>>,
}

impl Service for Plc {
    // `&self`, not `&mut self`: every connection shares one service, so your
    // mutable state lives behind your own lock.
    async fn on_request(
        &self,
        _conn: &Connection,
        _unit: UnitId,
        request: RequestPdu,
    ) -> Result<ResponsePdu, ExceptionCode> {
        let mut holding = self.holding.lock().expect("not poisoned");
        match request {
            RequestPdu::ReadHoldingRegisters { address, quantity } => {
                let registers = (0..quantity.0)
                    .map(|offset| {
                        let at = address.0.checked_add(offset)
                            .ok_or(ExceptionCode::IllegalDataAddress)?;
                        holding.get(&at).copied().map(RegisterValue)
                            .ok_or(ExceptionCode::IllegalDataAddress)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ResponsePdu::ReadHoldingRegisters { registers })
            }
            RequestPdu::WriteSingleRegister { address, value } => {
                holding.insert(address.0, value.0);
                Ok(ResponsePdu::WriteSingleRegister { address, value })
            }
            // Whatever this device does not implement. A refusal is stated in
            // Modbus' own vocabulary, so a transport error can never be
            // mistaken for an answer.
            _ => Err(ExceptionCode::IllegalFunction),
        }
    }
}

#[tokio::main]
async fn main() -> rust_modbus::Result<()> {
    let address: SocketAddr = "127.0.0.1:5030".parse().expect("a literal socket address");
    let listener = TcpListener::bind(address).await?;

    let server = Server::with_config(
        Plc::default(),
        ServerConfig { unit: Some(UnitId(1)) },
    );
    // `serve` consumes the server, so take the shutdown handle first.
    let handle = server.handle();
    let serving = tokio::spawn(server.serve(listener));

    // … later: returns once every in-flight handler has finished.
    handle.shutdown().await;
    let _ = serving.await;
    Ok(())
}
```

`serve` is the accept loop for a TCP listener, handling each connection
concurrently. `serve_link` runs one already-established stream instead, which is
how a serial line is served — and how the crate's own tests serve an in-memory
pipe. `ServerConfig::unit` defaults to `None`, meaning *answer every unit*; a
default of `Some(UnitId(1))` would silently drop every other unit's requests.

Beyond `on_request`, `Service` has three optional hooks with sensible defaults:
`on_connect` (answer `Acceptance::Reject` to close a peer unread), `on_disconnect`,
and `on_error`. `on_error` is separate because most per-request failures do not
end the connection.

## Examples

In [`examples/`](./examples/), runnable with `cargo run --example <name>`:

| Example | What it shows |
| --- | --- |
| `tcp_client` | Connect over TCP, read holding registers, write one, read it back. |
| `rtu_client` | The same over a real serial port. `--features rtu`; **needs hardware** or a `socat` pty pair. |
| `interop_server` | A TCP server backed by all four Modbus tables, printing every request. Used to point a foreign Modbus master at this crate. |

## Supported function codes

All nineteen public function codes of the Modbus Application Protocol
specification are named. The authoritative list is
[`docs/specs/frame/api-contract.md`](./docs/specs/frame/api-contract.md).

| Code | Function | Client method |
| --- | --- | --- |
| `0x01` | Read Coils | `read_coils` |
| `0x02` | Read Discrete Inputs | `read_discrete_inputs` |
| `0x03` | Read Holding Registers | `read_holding_registers` |
| `0x04` | Read Input Registers | `read_input_registers` |
| `0x05` | Write Single Coil | `write_single_coil` |
| `0x06` | Write Single Register | `write_single_register` |
| `0x07` | Read Exception Status † | `read_exception_status` |
| `0x08` | Diagnostics † | `diagnostics` |
| `0x0B` | Get Comm Event Counter † | `get_comm_event_counter` |
| `0x0C` | Get Comm Event Log † | `get_comm_event_log` |
| `0x0F` | Write Multiple Coils | `write_multiple_coils` |
| `0x10` | Write Multiple Registers | `write_multiple_registers` |
| `0x11` | Report Server ID † | `report_server_id` |
| `0x14` | Read File Record | `read_file_record` |
| `0x15` | Write File Record | `write_file_record` |
| `0x16` | Mask Write Register | `mask_write_register` |
| `0x17` | Read/Write Multiple Registers | `read_write_multiple_registers` |
| `0x18` | Read FIFO Queue | `read_fifo_queue` |
| `0x2B` | Encapsulated Interface Transport | `encapsulated_interface_transport` |

† The specification defines these for serial lines. The frame layer encodes and
decodes them over any framing; restricting them by transport is the client's or
server's judgment.

Function code 8 names fifteen diagnostic sub-functions, code 43 names the two
MEI types (CANopen General Reference, Read Device Identification), and nine
exception codes are named. Anything outside those sets is **not rejected** — it
is carried as a `Custom` / `Other` variant with an opaque body, so an unnamed
code round-trips rather than failing to decode. Issue one with `Client::call`.

The named set is a deliberate contract, not an open list; adding a name is a
specification change.

## Deliberate omissions

Honest about what this crate does not do, and why. Full reasoning in
[`PRD.md`](./PRD.md#non-goals).

- **No data model.** No register tables, no store, no built-in service — you
  implement `Service` (SV-R-005). What a coil or a register *means* is
  application state wearing a Modbus address, not protocol, and durability is
  even less this crate's business. See
  [`docs/specs/server/data-contract.md`](./docs/specs/server/data-contract.md).
- **No blocking / sync API.** Async-first on Tokio. A blocking facade is a
  separate product decision, not a mechanical addition.
- **No transport beyond TCP and RTU serial.** No UDP, and no RTU-over-TCP
  gateway emulation.
- **No ASCII *transport*.** ASCII framing exists at the frame layer for test
  fixtures and for comparing frames against upstream tooling by eye. Operating a
  serial port in ASCII mode is out of scope; the `AsciiClient` alias exists, but
  ASCII is not a supported operating mode.
- **No retry or reconnect in the client** (CL-R-033). A failed request surfaces
  the failure; what to do next depends on the installation, so it stays yours.
- **No device-specific quirk layer.** Vendor deviations from the standard are the
  consumer's problem.
- **No CLI, TUI or GUI.** This is a library only; it ships no binary.

## Documentation

`docs/specs/` is the **authoritative** specification of this crate's behavior —
the code is expected to conform to it, not the other way around. Requirement IDs
(`FR-R-*`, `CL-R-*`, `SV-R-*`, `TR-R-*`, `NF-R-*`) cited throughout the source
and in this README refer to it.

| Document | What it holds |
| --- | --- |
| [`docs/specs/`](./docs/specs/) | The normative specification, by capability area. Each area has a `requirements.md`, an `api-contract.md`, and an `edge-cases.md`. |
| [`docs/specs/frame/`](./docs/specs/frame/) | PDU/ADU encoding, function codes, exception responses, CRC-16, MBAP header. |
| [`docs/specs/client/`](./docs/specs/client/) | Request issuing, response matching, timeouts. |
| [`docs/specs/server/`](./docs/specs/server/) | Request dispatch, the `Service` trait, exception generation. |
| [`docs/specs/transport/`](./docs/specs/transport/) | TCP sockets, RTU serial ports, framing boundaries, connection lifecycle. |
| [`docs/specs/non-functional-requirements.md`](./docs/specs/non-functional-requirements.md) | Platforms, `no_std`, security posture, testing conventions. |
| [`PRD.md`](./PRD.md) | What the library is and is not for. |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | The module map, data flow, and concurrency model. |
| [`CONTRIBUTING.md`](./CONTRIBUTING.md) | Setup, the spec- and test-driven workflow, what to run before submitting. |
| [`tests/interop/README.md`](./tests/interop/README.md) | Interop against an independent Modbus implementation, in both directions. |

Each area's `edge-cases.md` records **known limitations** — behavior that is ugly
but intentional. Worth reading before filing something that looks like a bug.

## Testing

```sh
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo llvm-cov --all-features --fail-under-lines 80
```

Line coverage is gated at 80% in CI. Every listener in the suite binds port 0 and
reads the assigned port back, so tests never collide over a fixed port, and RTU
is exercised over in-memory duplex pairs rather than over hardware. Both roles
are additionally checked against an independent Modbus implementation — see
[`tests/interop/README.md`](./tests/interop/README.md).

## License

[MIT](./LICENSE).
