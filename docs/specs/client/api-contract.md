# Client — API Contract

The stable public surface owned by the client area: the client type and its
constructors, the request methods and their signatures, the configuration
fields, and the feature flags that gate them.

Per the ownership rule in [`../README.md`](../README.md), client configuration
fields are specified here; transport-level fields (baud rate, socket options)
belong to [`../transport/`](../transport/).

---

## 1. Client type and construction

One type for every framing (CL-R-001), built from a transport that is already
established (CL-R-002).

```rust
pub struct Client<S, F> { /* transport, config, next transaction id, state */ }

impl<S, F> Client<S, F>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    F: ClientFraming,
{
    pub fn new(transport: FrameTransport<S, F>) -> Self;
    pub fn with_config(transport: FrameTransport<S, F>, config: ClientConfig) -> Self;
    pub fn into_inner(self) -> FrameTransport<S, F>;
    pub fn is_desynchronized(&self) -> bool;
}

pub type TcpClient = Client<TcpStream, Tcp>;

#[cfg(feature = "rtu")]
pub type RtuClient = Client<SerialStream, Rtu>;
#[cfg(feature = "rtu")]
pub type AsciiClient = Client<SerialStream, Ascii>;
```

Every request method takes `&mut self`, which is how CL-R-005 is enforced: the
borrow checker permits one in-flight request, with no run-time flag to check.

`ClientFraming` is the seam of CL-R-003 — the one thing the three framings do
differently, named once:

```rust
pub trait ClientFraming: Framing {
    fn request_header(unit: UnitId, transaction: TransactionId) -> Self::Header;
    fn is_response_to(sent: &Self::Header, received: &Self::Header) -> bool;
    fn is_broadcast(unit: UnitId) -> bool;
}
```

`Rtu` and `Ascii` ignore the transaction identifier, match on the unit
identifier, and broadcast on unit 0 (CL-R-050). `Tcp` builds an `MbapHeader`,
matches on both of its fields (CL-R-020), and never broadcasts. The trait is
public because it bounds a public type, and unsealed because `Framing` is.

## 2. Request methods

Every method takes the unit identifier first (CL-R-003) and domain value types
throughout (CL-R-060, FR-R-007). All are `async` and return `Result`. A write
addressed to a broadcast identifier returns `Ok` without awaiting (CL-R-051); a
read so addressed fails without writing (CL-R-052).

| Method | Code | Arguments after `unit: UnitId` | Returns |
|---|---|---|---|
| `read_coils` | 1 | `address: Address, quantity: Quantity` | `Vec<bool>` |
| `read_discrete_inputs` | 2 | `address: Address, quantity: Quantity` | `Vec<bool>` |
| `read_holding_registers` | 3 | `address: Address, quantity: Quantity` | `Vec<RegisterValue>` |
| `read_input_registers` | 4 | `address: Address, quantity: Quantity` | `Vec<RegisterValue>` |
| `write_single_coil` | 5 | `address: Address, value: bool` | `()` |
| `write_single_register` | 6 | `address: Address, value: RegisterValue` | `()` |
| `read_exception_status` | 7 | — | `ExceptionStatus` |
| `diagnostics` | 8 | `sub_function: DiagnosticSubFunction, data: &[u16]` | `Vec<u16>` |
| `get_comm_event_counter` | 11 | — | `CommEventCounter` |
| `get_comm_event_log` | 12 | — | `CommEventLog` |
| `write_multiple_coils` | 15 | `address: Address, coils: &[bool]` | `()` |
| `write_multiple_registers` | 16 | `address: Address, registers: &[RegisterValue]` | `()` |
| `report_server_id` | 17 | — | `Vec<u8>` |
| `read_file_record` | 20 | `records: &[FileRecordRead]` | `Vec<FileRecordReadResponse>` |
| `write_file_record` | 21 | `records: &[FileRecordWrite]` | `()` |
| `mask_write_register` | 22 | `address: Address, and_mask: Mask, or_mask: Mask` | `()` |
| `read_write_multiple_registers` | 23 | `read_address: Address, read_quantity: Quantity, write_address: Address, registers: &[RegisterValue]` | `Vec<RegisterValue>` |
| `read_fifo_queue` | 24 | `address: Address` | `Vec<RegisterValue>` |
| `encapsulated_interface_transport` | 43 | `request: MeiRequest` | `MeiResponse` |

The write methods return `()` rather than the fields the server echoes: the echo
is not compared (CL-R-064), so returning it would invite a caller to compare it
by hand. A caller that wants the echo uses the raw path.

```rust
pub async fn call(&mut self, unit: UnitId, request: RequestPdu)
    -> Result<Option<ResponsePdu>>;
```

`call` is the escape hatch of CL-R-061: the response arrives as received,
exception responses included, so a custom function code (FR-R-011) is issuable
and an echo is inspectable. `None` means the request was a broadcast and no
reply was awaited (CL-R-053).

Two responses carry several values with no single natural payload, so each gets
a struct rather than a tuple:

```rust
pub struct CommEventCounter { pub status: u16, pub event_count: u16 }
pub struct CommEventLog {
    pub status: u16,
    pub event_count: u16,
    pub message_count: u16,
    pub events: Vec<u8>,
}
```

## 3. Configuration

```rust
pub struct ClientConfig {
    pub response_timeout: Duration,  // 1 s (CL-R-030)
}
```

One field, because CL-R-033 rules out retry and reconnect policy: those would be
configuration for behavior the client does not have. The RTU inter-frame
interval is *not* here — it is a serial-port property owned by the transport
area (TR-R-011).

## 4. Feature flags

| Feature | Default | Gates |
|---|---|---|
| `std` | on | the whole client area (CL-R-004) |
| `rtu` | off | `RtuClient` and `AsciiClient` only |

The client is generic over the stream, so `Client<S, Rtu>` over an in-memory
duplex pair works with the `rtu` feature off; only the alias naming a serial
port is gated.

## 5. Error variants

Added by this area, all gated on `std`:

| Variant | Fields | Requirements |
|---|---|---|
| `Exception` | `function: FunctionCode, exception: ExceptionCode` | CL-R-040, CL-R-041 |
| `UnexpectedFunction` | `expected: FunctionCode, actual: FunctionCode` | CL-R-022 |
| `Desynchronized` | — | CL-R-031, CL-R-032 |

`Timeout { what: "response" }` is the transport area's existing variant reused,
not a fourth one: a caller distinguishes a response timeout from a connect
timeout by the field, and CL-R-031 means the client is desynchronized either way.
