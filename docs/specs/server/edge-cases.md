# Server — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**.
Entries under "Known limitations" are working as implemented; they are recorded
here so they are not mistaken for oversights and silently "fixed".

---

## 1. Request handling

| Condition | Behavior |
|---|---|
| A request the service refuses | Exception response to the requested function, with the service's code (SV-R-012) |
| An unsupported function code the frame area names | Decoded and dispatched; refusing it is the service's call (SV-R-005) |
| A function code the frame area cannot decode, TCP | `InvalidFunctionCode`; reported to `on_error`, connection ends (SV-R-050) |
| A function code the frame area cannot decode, RTU or ASCII | `InvalidFunctionCode`; reported to `on_error`, no response, serving continues (SV-R-050) |
| Address or quantity outside what the wire format permits | The frame area's decode error; treated as undecodable (SV-R-050) |
| Address or quantity the *application* does not have | Nothing the server can judge — the service returns `IllegalDataAddress` (SV-R-005) |
| A write to what the consumer considers read-only | The service's refusal; the crate has no read-only notion (SV-R-005) |
| Unit identifier does not match a configured one | No response at all, connection continues (SV-R-021) |
| A request to a non-matching unit id over RTU-over-TCP | No response, connection stays open (SV-R-021) — the socket is one gateway, but the bus behind it is many devices, so the identifier is a real address and answering for another device would corrupt its exchange |
| Unit identifier when none is configured | Dispatched as received; the service decides (SV-R-022) |
| Unit 0 on RTU or ASCII | Dispatched, never answered (SV-R-023) |
| The service returns a response that cannot be encoded | `on_error`, no response sent, connection continues (SV-R-014) |
| The service returns another function's response | Sent as returned; the server does not check (SV-R-013) |

A service refusal and a decode failure are deliberately different channels: the
first is a Modbus answer, the second is not answerable at all, because the frame
that would say which function was being refused is the frame that could not be
trusted. Whether the connection survives that failure depends on the framing: on
TCP and on RTU-over-TCP the reader is left unsure where the next frame starts and
the connection ends, while RTU and ASCII locate the next boundary from the wire
itself and serving continues (SV-R-050, FR-R-144).

## 2. Connections

| Condition | Behavior |
|---|---|
| Bind failure | Never reaches the server: binding belongs to the transport area (TR-R-030) |
| A peer that connects and sends nothing | Held open, one task idle, until it closes or shutdown; there is no idle timeout |
| A peer that closes between ADUs | `Disconnect::Closed` (SV-R-052) |
| A peer that closes mid-ADU | `ConnectionClosed` to `on_error`, `Disconnect::Failed` (TR-R-014, SV-R-051) |
| `on_connect` returns `Acceptance::Reject` | Closed with no request read, `Disconnect::Rejected` (SV-R-032) |
| One connection fails | Others unaffected, accepting continues (SV-R-035) |
| Accept itself fails | Serving returns the error; connections already running are drained first (SV-R-051) |
| A request in flight at shutdown | Runs to completion and its response is sent (SV-R-042) |
| An idle connection at shutdown | Closed without waiting for a request, `Disconnect::ShuttingDown` (SV-R-043) |

## 3. Known limitations

- **A corrupt line produces one error per frame, indefinitely.** On RTU or ASCII
  the server reports every undecodable frame to `on_error` and keeps serving, by
  design (SV-R-050) — one frame of noise must not take a device off the bus. A
  line that is permanently corrupt therefore produces a steady stream of callbacks
  rather than a closed connection. A service that wants to give up counts them
  itself, where it knows its own tolerance; the crate cannot pick a threshold that
  is right for both a quiet bench and a noisy plant floor.
- **No connection limit.** Every accepted connection gets a task, and nothing
  caps how many. A service that wants a cap enforces it in `on_connect`
  (SV-R-032), where it has the peer address and its own count — the server cannot
  choose a number that is right for both an embedded gateway and a SCADA front
  end.
- **No idle or per-request timeout.** A connection that goes quiet is not closed,
  and a slow `on_request` is awaited indefinitely. A responder that times out its
  own handler would answer nothing while the handler still ran, which is worse
  than being slow; a service that needs a bound applies `tokio::time::timeout`
  inside `on_request`, where it knows what the operation costs.
- **Requests on one connection are handled one at a time.** Modbus TCP permits an
  initiator to pipeline transactions; this server reads, dispatches, answers, and
  only then reads again. Concurrency is per connection (SV-R-030), which matches
  how initiators behave in practice — including this crate's own client
  (CL-R-005). Pipelining would need out-of-order responses and its own ordering
  contract.
- **No data model at all.** See [`data-contract.md`](./data-contract.md). This is
  the largest deliberate omission in the crate: it means "hello world" for this
  server is an `impl Service` of a dozen lines, not two.
- **The service is moved into the server, not borrowed.** SV-R-002 takes
  ownership, and the orphan rule stops a consumer implementing `Service` for
  `Arc<their type>`. So a consumer that also wants to read its own store keeps its
  state in `Arc` fields and clones the service. Borrowing instead would tie the
  server's lifetime to a scope, which a `'static` connection task cannot have.
- **A rejected connection is closed, not refused.** `on_connect` runs after the
  TCP handshake completed, so a refused peer sees a connection that opens and
  immediately closes. Refusing before the handshake is not something a listener
  can express.
- **Shutdown is cooperative, not immediate.** SV-R-044 waits for handlers. A
  service whose `on_request` never returns keeps `shutdown()` pending forever; the
  bound belongs to the handler, as above.
