# Transport — API Contract

The stable public surface owned by the transport area: the transport
abstraction, the TCP and RTU implementations, and every configuration field that
controls a socket or a serial port.

Per the ownership rule in [`../README.md`](../README.md), serial parameters and
socket options are specified here, not in the areas that happen to expose them.

---

## 1. Transport abstraction

The seam is a **generic bound, not a trait of our own**: anything that is an
async duplex byte stream serves (TR-R-001), so `tokio::io::duplex` substitutes
for a socket or a serial port in tests without a shim. A `Transport` trait would
add dyn-safety and `async_trait` cost while naming nothing the bound does not.

```rust
pub struct FrameTransport<S, F> { /* stream, read buffer, config */ }

impl<S, F> FrameTransport<S, F>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    F: Framing,
{
    pub fn new(stream: S) -> Self;
    pub fn with_config(stream: S, config: TransportConfig) -> Self;

    pub async fn send_request(&mut self, header: &F::Header, pdu: &RequestPdu) -> Result<()>;
    pub async fn recv_request(&mut self) -> Result<(F::Header, RequestPdu)>;
    pub async fn send_response(&mut self, header: &F::Header, pdu: &ResponsePdu) -> Result<()>;
    pub async fn recv_response(&mut self) -> Result<(F::Header, ResponsePdu)>;

    pub fn into_inner(self) -> S;
}
```

The four coding methods mirror the frame area's direction-explicit rule
(FR-R-005): a PDU is not self-describing, so the caller states the direction. A
client sends requests and receives responses; a server does the reverse. That is
the whole of the difference between the roles at this layer (TR-R-002).

`TransportConfig` carries what boundary detection needs and nothing else — the
RTU inter-frame interval of TR-R-011, derived from a `SerialConfig` or set
directly. TCP, RTU-over-TCP and ASCII boundaries are found in the bytes and
ignore it (TR-R-048).

```rust
pub struct TransportConfig { pub inter_frame_interval: Duration }  // 2.005 ms

impl TransportConfig {
    pub fn from_serial(config: &SerialConfig) -> Result<Self>;
}

impl SerialConfig {
    pub fn inter_frame_interval(&self) -> Result<Duration>;
}
```

The interval lives on `SerialConfig` as well as on `TransportConfig`, so the
3.5-character-time rule is computable — and testable — with the `rtu` feature
off and no port present. `open_serial` derives one from the other, so a port and
its timing cannot disagree.

## 2. TCP configuration

```rust
pub type TcpTransport = FrameTransport<TcpStream, Tcp>;
pub type RtuOverTcpTransport = FrameTransport<TcpStream, RtuOverTcp>;

pub struct TcpConfig {
    pub connect_timeout: Duration,  // 5 s (TR-R-021)
    pub nodelay: bool,              // true (TR-R-022)
}

pub async fn connect_tcp(addr: SocketAddr, config: TcpConfig) -> Result<TcpTransport>;
pub async fn connect_tcp_framed<F: Framing>(
    addr: SocketAddr,
    config: TcpConfig,
) -> Result<FrameTransport<TcpStream, F>>;

pub struct TcpListener { /* … */ }

impl TcpListener {
    pub async fn bind(addr: SocketAddr) -> Result<Self>;
    pub fn local_addr(&self) -> Result<SocketAddr>;
    pub async fn accept(&self) -> Result<(TcpTransport, SocketAddr)>;
    pub async fn accept_framed<F: Framing>(&self)
        -> Result<(FrameTransport<TcpStream, F>, SocketAddr)>;
}
```

`connect_tcp` and `accept` are `connect_tcp_framed::<Tcp>` and
`accept_framed::<Tcp>` under their existing names (TR-R-024), kept so the common
case needs no turbofish and no existing call site changes. A gateway link is
`connect_tcp_framed::<RtuOverTcp>`, which gets the connect timeout and the
`TCP_NODELAY` default of TR-R-021 and TR-R-022 unchanged — nothing about
establishing the socket differs, only what is read off it.

`local_addr` exists so a test can bind port 0 and read the assigned port back,
which the testing conventions require of every listener.

## 3. RTU serial configuration

```rust
pub struct SerialConfig {
    pub baud_rate: u32,             // 19200
    pub data_bits: DataBits,        // Eight
    pub parity: Parity,             // Even
    pub stop_bits: StopBits,        // One
    pub flow_control: FlowControl,  // None
}

pub enum DataBits { Five, Six, Seven, Eight }
pub enum Parity { None, Odd, Even }
pub enum StopBits { One, Two }
pub enum FlowControl { None, Software, Hardware }

#[cfg(feature = "rtu")]
pub type SerialTransport<F> = FrameTransport<SerialStream, F>;

#[cfg(feature = "rtu")]
pub fn open_serial<F: Framing>(path: &str, config: SerialConfig) -> Result<SerialTransport<F>>;
```

The defaults are the Modbus serial-line defaults (TR-R-031). The enums are the
crate's own rather than re-exported from the serial backend, so the backend is
not part of the public API and the types exist with the `rtu` feature off — the
inter-frame interval of TR-R-011 is computed from them, which the ASCII and TCP
paths never need but the pure calculation is testable without a serial port.

`open_serial` is generic over the framing because a serial line carries RTU or
ASCII framing at the operator's choice, over identical port settings.

## 4. Feature flags

| Feature | Default | Gates |
|---|---|---|
| `std` | on | everything outside the frame area (NF-R-002) |
| `rtu` | **off** | `open_serial` and the serial backend (TR-R-032) |

`rtu` implies `std`. The frame area's RTU *framing* is not gated: encoding an RTU
ADU is pure computation and stays available on `no_std`. Only opening a physical
port is gated.

## 5. Error variants

Added by this area, all gated on `std`:

| Variant | Fields | Requirements |
|---|---|---|
| `Io` | `kind: std::io::ErrorKind` | TR-R-040 |
| `Timeout` | `what: &'static str` | TR-R-021, TR-R-041 |
| `ConnectionClosed` | — | TR-R-014 |
| `Configuration` | `field: &'static str` | TR-R-031 |

`Io` carries the `ErrorKind` rather than the `std::io::Error` because `Error`
derives `PartialEq`, which `io::Error` does not implement; the kind is the part a
caller matches on, and preserving the OS message would cost every existing
equality assertion in the crate.
