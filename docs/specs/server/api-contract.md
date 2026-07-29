# Server — API Contract

The stable public surface owned by the server area: the server type and its
constructors, the trait a consumer implements to answer requests, the shutdown
handle, the configuration fields, and the feature flags that gate them.

Per the ownership rule in [`../README.md`](../README.md), server configuration
fields are specified here; transport-level fields (socket options, baud rate)
belong to [`../transport/`](../transport/).

---

## 1. Server type and construction

One type for every framing (SV-R-001), built from a service (SV-R-002).

```rust
pub struct Server<S> { /* Arc<Service>, config, shutdown signal */ }

impl<S: Service> Server<S> {
    pub fn new(service: S) -> Self;
    pub fn with_config(service: S, config: ServerConfig) -> Self;
    pub fn handle(&self) -> ServerHandle;

    pub async fn serve(self, listener: TcpListener) -> Result<()>;

    pub async fn serve_link<T, F>(self, transport: FrameTransport<T, F>) -> Result<()>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send,
        F: ServerFraming;
}
```

`ServerFraming` is the seam of SV-R-001 — the mirror of the client's
`ClientFraming`, since a responder reads a header rather than building one:

```rust
pub trait ServerFraming: Framing {
    fn unit(header: &Self::Header) -> UnitId;
    fn is_broadcast(unit: UnitId) -> bool;
}
```

`Rtu` and `Ascii` take the header itself as the unit and broadcast on unit 0
(FR-R-096, FR-R-117); `Tcp` takes the MBAP header's unit field and never
broadcasts. Public because it bounds a public method, unsealed because `Framing`
is.

`serve` accepts connections and handles each concurrently (SV-R-030);
`serve_link` runs one already-established transport, which is how a serial line
is served (SV-R-007). Both consume the server, so the handle of SV-R-040 is taken
first:

```rust
let server = Server::with_config(my_service, ServerConfig { unit: Some(UnitId(1)) });
let handle = server.handle();
let serving = tokio::spawn(server.serve(listener));
// …
handle.shutdown().await;      // returns once every handler has finished
```

The address a listener is bound to is read back through the transport area's
`TcpListener::local_addr`, not through the server: the server never binds, so it
never owns the address.

## 2. The service trait

The one trait a consumer implements (SV-R-003). Only `on_request` is required
(SV-R-004).

```rust
pub trait Service: Send + Sync + 'static {
    fn on_request(
        &self,
        conn: &Connection,
        unit: UnitId,
        request: RequestPdu,
    ) -> impl Future<Output = core::result::Result<ResponsePdu, ExceptionCode>> + Send;

    fn on_connect(&self, conn: &Connection) -> impl Future<Output = Acceptance> + Send {
        async { Acceptance::Accept }
    }

    fn on_disconnect(&self, conn: &Connection, reason: Disconnect)
        -> impl Future<Output = ()> + Send { async {} }

    fn on_error(&self, conn: &Connection, error: &Error)
        -> impl Future<Output = ()> + Send { async {} }
}
```

`&self`, not `&mut self`: that is how SV-R-003 is enforced in the type system —
concurrent connections hold the same service, so mutable state lives behind the
implementor's own lock. `Send + Sync + 'static` is what lets a connection be
handled in its own task.

Returning `impl Future<…> + Send` rather than declaring `async fn` is deliberate:
an `async fn` in a trait does not promise a `Send` future, and without that
promise a connection cannot be spawned. The trait is consequently not
object-safe; `Server<S>` is generic over it, which is what shared, concurrent
dispatch wants in any case. It is unsealed.

A server **owns** its service (SV-R-002), and the orphan rule forbids a consumer
from writing `impl Service for Arc<MyType>` outside this crate. A consumer that
wants to keep a view of its own state therefore implements `Service` for a type
that is cheap to clone and *shares* when cloned — state behind `Arc<Mutex<…>>`
fields — and hands one clone to `Server::new`. That is the shape
`tests/server_tcp.rs` demonstrates.

`on_request` returns `Result<ResponsePdu, ExceptionCode>`: a refusal is expressed
in the protocol's own vocabulary (SV-R-012), so a service cannot accidentally
answer a Modbus request with a transport error. `on_connect` answers with an
`Acceptance`, not a `bool` (SV-R-032) — `Acceptance::Reject` reads the same way at
the call site as in the signature, where `false` would have to be remembered. `on_error` is separate from `on_disconnect` because most
per-request failures do not end the connection (SV-R-034).

```rust
pub struct Connection { /* id, peer */ }

impl Connection {
    pub fn id(&self) -> ConnectionId;
    pub fn peer(&self) -> Option<SocketAddr>;   // None on a serial link
}

pub struct ConnectionId(pub u64);   // a domain value type, per FR-R-007

pub enum Acceptance {
    Accept,           // serve the connection
    Reject,           // close it unread (SV-R-032)
}

pub enum Disconnect {
    Closed,           // the peer closed between two ADUs (SV-R-052)
    Rejected,         // on_connect answered Reject (SV-R-032)
    Failed(Error),    // an I/O failure or an undecodable request (SV-R-050)
    ShuttingDown,     // the handle asked (SV-R-043)
}
```

`ConnectionId` is a `u64` and not the peer address: an address is reused as soon
as a socket closes, and a serial link has none (SV-R-031).

## 3. What this area does not expose

No data store, no register tables, no built-in service (SV-R-005). See
[`data-contract.md`](./data-contract.md) for why.

## 4. Shutdown handle

```rust
pub struct ServerHandle { /* … */ }

impl ServerHandle {
    pub async fn shutdown(&self);
    pub fn is_shutting_down(&self) -> bool;
}

impl Clone for ServerHandle {}
```

`shutdown` is idempotent and awaits the drain of SV-R-044. It is `Clone` so that
several tasks — a signal handler and a test, say — may hold one.

## 5. Configuration and feature flags

```rust
pub struct ServerConfig {
    pub unit: Option<UnitId>,   // None (SV-R-008, SV-R-022)
}
```

`None` by default: a server that answers only unit 1 is a deliberate
configuration, and a default of `Some(1)` would silently drop every other unit's
requests (SV-R-021).

| Feature | Default | Gates |
|---|---|---|
| `std` | on | the whole server area (SV-R-006) |
| `rtu` | off | nothing in this area |

`serve_link` is generic over the stream, so a server over an in-memory duplex
pair or over `Rtu` framing needs no feature beyond `std`; only opening a real
serial port is gated.

## 6. Error variants

This area adds none. A service refusal is an `ExceptionCode`, not an `Error`;
connection failures surface as the frame and transport areas' existing variants,
carried to the consumer through `on_error` and `Disconnect::Failed`.
