# Modbus Library in Rust

Async Modbus **client** and **server** for Rust, over **Modbus TCP**, **Modbus
RTU** serial, **TLS** (encrypted TCP), and **UDP**.

Every combination below is first-class, and none is an afterthought:

|                          | TCP | TLS over TCP | RTU serial | UDP |
| ------------------------ | :-: | :----------: | :--------: | :-: |
| **Client** (initiator)   | ✅  |      ✅      |     ✅     | ✅  |
| **Server** (responder)   | ✅  |      ✅      |     ✅     | ✅  |

Both roles sit on one shared frame layer, so a fix in encoding benefits both and
the two cannot drift apart. Modbus **ASCII** framing is encodable and decodable
too, but only as a frame format — see [Deliberate omissions](#deliberate-omissions).

Two more transports build on the table above without adding a new role:
**RTU-over-TCP**, for RS-485-to-Ethernet gateways that forward the raw RTU ADU
over a socket (see [Transparent gateways](#transparent-gateways)), and
**RS-485 kernel direction control** on Linux, for half-duplex serial adapters
driven by `TIOCSRS485` (see [RS-485 direction control](#rs-485-direction-control)).

- Async-first on [Tokio](https://tokio.rs). No thread pool, no blocking bridge.
- **Typed, not stringly.** Addresses, quantities, register values and unit
  identifiers are distinct types that share a width but cannot be swapped at a
  call site. Every failure is a variant of one error enum.
- **Robust against hostile input.** Truncated, malformed or oversized frames
  produce a typed error — never a panic, an out-of-bounds slice, or an unbounded
  allocation. `#![forbid(unsafe_code)]`.
- **A bad frame costs one frame.** On a bus that delimits its frames — RTU by
  silence, ASCII by `:` and CRLF — the next boundary is still on the wire after
  a frame fails to decode, so line noise costs that exchange and nothing more: a
  client stays synchronized, and a server stays on the bus rather than dropping
  the link (FR-R-144). Modbus TCP carries its length in the frame, and
  RTU-over-TCP derives its length from the frame's own fields, so on either of
  those a frame that cannot be decoded does take the stream's alignment with it.
- **Allocation-free in steady state.** Every encode appends into a buffer the
  caller owns, and a transport reuses one buffer across frames, so sending after
  the first frame allocates nothing at all (NF-R-009). An owning `encode` is
  still there when a `Vec` is what you wanted.
- **`no_std` + `alloc`** with default features off: the frame layer needs no
  operating system.
- **Testable without hardware.** Transports are a generic bound over any async
  duplex stream, so an in-memory pipe substitutes for a socket or a serial port.

## Install

```sh
cargo add rust-modbus
# for RTU serial ports:
cargo add rust-modbus --features rtu
# for TLS:
cargo add rust-modbus --features tls
```

### Feature flags

| Feature | Default | What it gates |
| --- | --- | --- |
| `std` | **on** | Everything above the frame layer: `Client`, `Server`, `FrameTransport`, TCP, UDP. Pulls in Tokio. |
| `rtu` | off | Opening a real serial port: `open_serial`, `SerialTransport`, `RtuClient`, `AsciiClient`. Implies `std`. |
| `rs485` | off | RS-485 kernel direction control (`TIOCSRS485`) on Linux. Implies `rtu`. |
| `sync` | off | The blocking client: `SyncClient` and its aliases. Implies `std`. |
| `tls` | off | TLS transport over TCP: `connect_tls`, `TlsListener`. Implies `std`. |
| `serde` | off | `Serialize`/`Deserialize` on domain value types and configuration types. |

`rtu` is off by default so a TCP-only consumer acquires no serial dependency. It
gates *only opening a port* — RTU and ASCII **framing** are always available, so
`Client<FrameTransport<S, Rtu>, Rtu>` over any duplex stream (an in-memory pipe,
a socket, a pty) works with the feature off. That is how this crate tests RTU in
CI. UDP needs no feature beyond `std`: unlike a serial port, opening a socket
carries no extra dependency.

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

### Over UDP

`UdpClient` is `Client` built over `connect_udp`'s output instead of a TCP
stream — every request method above works identically:

```rust,no_run
use rust_modbus::{Address, Client, Quantity, UdpClient, UdpConfig, UnitId, connect_udp};

#[tokio::main]
async fn main() -> rust_modbus::Result<()> {
    let address = "127.0.0.1:502".parse().expect("a literal socket address");
    let transport = connect_udp(address, UdpConfig::default()).await?;
    let mut client: UdpClient = Client::new(transport);

    let registers = client
        .read_holding_registers(UnitId(1), Address(0), Quantity(4))
        .await?;
    println!("{registers:?}");
    Ok(())
}
```

UDP carries one ADU per datagram with no transport-level retransmission,
sequencing, or fragmentation handling (TR-R-070) — the client's own response
timeout is what notices a dropped datagram, same as it notices a slow TCP peer.

### Over TLS

Enable the `tls` feature. `connect_tls` performs a TCP connect, then a TLS
handshake, then hands back a `FrameTransport` — everything past that point is
the same `Client`:

```rust,no_run
use rust_modbus::{
    Address, Client, Quantity, RootStore, ServerCertVerification, TcpConfig, TlsClientConfig,
    UnitId, connect_tls,
};

#[tokio::main]
async fn main() -> rust_modbus::Result<()> {
    let address = "127.0.0.1:802".parse().expect("a literal socket address");
    let tls_config = TlsClientConfig {
        // `RootStore::default()` trusts the platform's native roots.
        server_cert: ServerCertVerification::Verify(RootStore::default()),
        client_identity: None,
    };
    let transport = connect_tls(address, TcpConfig::default(), tls_config).await?;
    let mut client = Client::new(transport);

    let registers = client
        .read_holding_registers(UnitId(1), Address(0), Quantity(4))
        .await?;
    println!("{registers:?}");
    Ok(())
}
```

`TlsClientConfig::client_identity` presents a client certificate if the server
requests one (`ClientCertPolicy::Require`). `ServerCertVerification` has no
boolean spelling for "skip verification" — the escape hatch is named
`DangerousDisableVerification` so it cannot pass unnoticed in a diff.

### Without async

Enable the `sync` feature. `SyncClient` owns a runtime and mirrors every request
method of `Client` with `async` removed — same arguments, same return types, same
timeout, exception and desynchronization behavior, because it delegates rather
than reimplements.

```rust
use rust_modbus::{Address, ClientConfig, Quantity, SyncTcpClient, TcpConfig, UnitId};

fn main() -> rust_modbus::Result<()> {
    let mut client = SyncTcpClient::connect(
        "127.0.0.1:502".parse().expect("a literal address"),
        TcpConfig::default(),
        ClientConfig::default(),
    )?;

    let registers = client.read_holding_registers(UnitId(1), Address(0), Quantity(2))?;
    println!("{registers:?}");
    Ok(())
}
```

Call it from a thread that has no runtime. Calling it from inside one would
deadlock, so it returns `Error::BlockingInAsyncContext` instead of trying — if
you have a runtime, you want `Client`. There is no `into_inner` and no blocking
server; see [Deliberate omissions](#deliberate-omissions).

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

### Over UDP

`serve_udp` answers datagrams on an already-bound socket. There is no
connection to accept — a UDP peer is stateless, so this reuses the same
`Service` written above with no lifecycle hooks called:

```rust
let socket = tokio::net::UdpSocket::bind("127.0.0.1:502").await?;
server.serve_udp(socket).await?;
```

### Over TLS

Enable the `tls` feature. `TlsListener::bind` takes a `TlsServerConfig` (a
certificate chain, its private key, and a `ClientCertPolicy`) and performs the
handshake per accepted connection before yielding a `FrameTransport` — the rest
of the accept loop is the plain-TCP one, hand-rolled or via `serve_link`:

```rust
use rust_modbus::{ClientCertPolicy, TlsListener, TlsServerConfig, load_pem_cert_chain, load_pem_private_key};

let config = TlsServerConfig {
    cert_chain: load_pem_cert_chain(include_bytes!("server.pem"))?,
    key: load_pem_private_key(include_bytes!("server.key"))?,
    client_certs: ClientCertPolicy::None,
};
let listener = TlsListener::bind("127.0.0.1:802".parse().expect("a literal address"), config).await?;
let (transport, _peer_addr, _client_cert) = listener.accept().await?;
```

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

## Transparent gateways

Many RS-485-to-Ethernet converters do not speak Modbus TCP. They forward the RTU
ADU — address, PDU, CRC — verbatim over a socket, with no MBAP header. That is
`RtuOverTcp` framing:

```rust
let transport = connect_tcp_framed::<RtuOverTcp>(addr, TcpConfig::default()).await?;
let mut client = RtuOverTcpClient::new(transport);
let regs = client.read_holding_registers(UnitId(1), Address(0), Quantity(2)).await?;
```

The wire format is RTU's, byte for byte. What differs is where a frame ends: a
socket has no inter-frame silence to observe, so the length is derived from the
direction, the function code, and the byte-count fields the frame carries
(FR-R-146). Two consequences worth knowing before you deploy it:

- **Function codes 8, 43 outside MEI type 14, and custom codes are not reachable
  this way.** Their length is not derivable from their content, and this crate
  refuses with `Error::IndeterminateLength` rather than guess (FR-R-148).
- **A bad frame costs the connection.** The boundary is read out of the frame, so
  a frame that is wrong loses the next one's position too (FR-R-150). Reconnect;
  do not retry on the same socket.

No `rtu` feature is needed — nothing here opens a serial port.

## RS-485 direction control

Enable the `rs485` feature (implies `rtu`; Linux only). Many RS-485 adapters need
the kernel told when to assert RTS for transmission — this configures that
directly, with no application-driven GPIO hook:

```rust
use core::time::Duration;
use rust_modbus::{Rs485Config, RtsPolarity, SerialConfig};

let config = SerialConfig {
    rs485: Some(Rs485Config {
        rts_on_send: RtsPolarity::High,
        delay_before_send: Duration::from_millis(0),
        delay_after_send: Duration::from_millis(0),
    }),
    ..SerialConfig::default()
};
```

`open_serial` issues the `TIOCSRS485` ioctl with this configuration before
returning the transport, so a caller never holds one whose direction control
silently failed to apply. On a non-Linux target, or a driver that does not
implement the ioctl, opening fails with a typed error rather than returning a
transport that would then hang mid-transmission.

## Deliberate omissions

Honest about what this crate does not do, and why. Full reasoning in
[`PRD.md`](./PRD.md#non-goals).

- **No data model.** No register tables, no store, no built-in service — you
  implement `Service` (SV-R-005). What a coil or a register *means* is
  application state wearing a Modbus address, not protocol, and durability is
  even less this crate's business. See
  [`docs/specs/server/data-contract.md`](./docs/specs/server/data-contract.md).
- **No interpretation of register contents.** No `f32` helpers, no word-order or
  endianness options above the register, no scaling or unit conversion. Modbus
  defines nothing wider than a 16-bit register, so combining two into a float is
  a device convention — the four incompatible orders in the field are the proof —
  and it belongs with the register map, not the protocol. Byte order *within* a
  register is protocol and is implemented (big-endian on the wire); order
  *across* registers is yours.
- **No blocking *server*.** A blocking *client* ships behind the off-by-default
  `sync` feature: `SyncClient` owns a runtime and mirrors every request method of
  the async client, with identical timeout, desynchronization and broadcast
  behavior. A server is driven by inbound connections rather than by
  caller-issued calls, so the thread structure that would serve it is yours to
  choose. The blocking client also has no `into_inner`, since the transport it
  would hand back needs a runtime the caller does not have.
- **No transport beyond TCP, RTU serial, RTU-over-TCP, UDP, and TLS-over-TCP**
  unless later specified. RTU-over-TCP (see
  [Transparent gateways](#transparent-gateways)) trades a per-frame boundary for
  a per-connection one — a bad frame there costs the link, not just the frame,
  since the boundary comes out of each frame's own length fields. UDP support is
  MBAP framing only; there is no raw-PDU-over-UDP mode, and no transport-level
  retransmission, sequencing, or fragmentation handling.
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
| [`docs/specs/transport/`](./docs/specs/transport/) | TCP, TLS, UDP sockets, RTU serial ports and RS-485 direction control, framing boundaries, connection lifecycle. |
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
