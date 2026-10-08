# Server — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**. Entries under "Known limitations" are working as implemented; they are recorded here so they are not mistaken for oversights and silently "fixed".

IDs are stable and append-only (`SV-E-nnn`). See [`../README.md`](../README.md).

---

## Request handling

| ID | Condition | Behavior |
|---|---|---|
| **SV-E-001** | A request the service refuses | Exception response to the requested function, with the service's code (SV-R-012) |
| **SV-E-002** | An unsupported function code the frame area names | Decoded and dispatched; refusing it is the service's call (SV-R-005) |
| **SV-E-003** | A function code the frame area cannot decode, TCP | `InvalidFunctionCode`; reported to `on_error`, connection ends (SV-R-050) |
| **SV-E-004** | A function code the frame area cannot decode, RTU or ASCII | `InvalidFunctionCode`; reported to `on_error`, no response, serving continues (SV-R-050, SV-R-065) |
| **SV-E-005** | Address or quantity outside what the wire format permits | The frame area's decode error; treated as undecodable (SV-R-050) |
| **SV-E-006** | Address or quantity the *application* does not have | Nothing the server can judge — the service returns `IllegalDataAddress` (SV-R-005) |
| **SV-E-007** | A write to what the consumer considers read-only | The service's refusal; the crate has no read-only notion (SV-R-005) |
| **SV-E-008** | Unit identifier does not match a configured one | No response at all, connection continues (SV-R-021) |
| **SV-E-009** | `on_request` returns `Ok(None)` for a matched, non-broadcast unit id | No response sent, connection continues (SV-R-024) — same channel as broadcast/unit-mismatch, decided by the service |
| **SV-E-010** | A request to a non-matching unit id over RTU-over-TCP | No response, connection stays open (SV-R-021) — the socket is one gateway, but the bus behind it is many devices, so the identifier is a real address and answering for another device would corrupt its exchange |
| **SV-E-011** | Unit identifier when none is configured | Dispatched as received; the service decides (SV-R-022) |
| **SV-E-012** | Unit 0 on RTU or ASCII | Dispatched, never answered (SV-R-023) |
| **SV-E-013** | The service returns a response that cannot be encoded | `on_error`, no response sent, connection continues (SV-R-014) |
| **SV-E-014** | The service returns another function's response | Sent as returned; the server does not check (SV-R-013) |

A service refusal and a decode failure are deliberately different channels: the first is a Modbus answer, the second is not answerable at all, because the frame that would say which function was being refused is the frame that could not be trusted. Whether the connection survives that failure depends on the framing: on TCP and on RTU-over-TCP the reader is left unsure where the next frame starts and the connection ends, while RTU and ASCII locate the next boundary from the wire itself and serving continues (SV-R-050, SV-R-065, FR-R-144).

## Connections

| ID | Condition | Behavior |
|---|---|---|
| **SV-E-015** | Bind failure | Never reaches the server: binding belongs to the transport area (TR-R-030) |
| **SV-E-016** | A peer that connects and sends nothing | Held open, one task idle, until it closes or shutdown; there is no idle timeout |
| **SV-E-017** | A peer that closes between ADUs | `Disconnect::Closed` (SV-R-052) |
| **SV-E-018** | A peer that closes mid-ADU | `ConnectionClosed` to `on_error`, `Disconnect::Failed` (TR-R-088, SV-R-051) |
| **SV-E-019** | `on_connect` returns `Acceptance::Reject` | Closed with no request read, `Disconnect::Rejected` (SV-R-032) |
| **SV-E-020** | One connection fails | Others unaffected, accepting continues (SV-R-035) |
| **SV-E-021** | Accept itself fails | Reported to on_accept_error. Stop (default): connections already running are drained, serving returns the error. Continue: connections kept, accept retried once the notification completes (SV-R-059, SV-R-060, SV-R-067) |
| **SV-E-022** | Shutdown while on_accept_error is pending | Notification future dropped; shutdown proceeds as during accept (SV-R-061) |
| **SV-E-037** | Receiving from the socket fails under serve_udp | Reported to on_receive_error. Default: Continue on Transient, Stop otherwise. Continue: in-flight datagrams kept, receive retried once the notification completes. Stop: in-flight datagrams finish, serve_udp returns the error (SV-R-073, SV-R-074, SV-R-077, SV-R-078) |
| **SV-E-038** | Shutdown while on_receive_error is pending | Notification future dropped; in-flight datagrams finish; serve_udp returns Ok(()) (SV-R-079) |
| **SV-E-039** | Windows reports WSAECONNRESET on receive after an ICMP port-unreachable for an earlier reply | Transient (TR-R-105), so the default on_receive_error answers Continue and serving goes on (SV-R-074) |
| **SV-E-042** | A peer sends a datagram larger than the receive buffer under serve_udp | Windows: the receive fails with WSAEMSGSIZE (10040), Transient (TR-R-105), so the default on_receive_error answers Continue and serving goes on (SV-R-074, SV-R-058). Unix: the OS truncates the datagram and it is handled like any other; one that fails to decode never reaches on_receive_error (SV-R-075) |
| **SV-E-023** | A request in flight at shutdown | Runs to completion and its response is sent (SV-R-042) |
| **SV-E-024** | An idle connection at shutdown | Closed without waiting for a request, `Disconnect::ShuttingDown` (SV-R-043) |
| **SV-E-025** | serve_link's link fails (I/O error, e.g. a serial adapter unplugged) | on_error, then on_disconnect with Disconnect::Failed(error); serve_link then returns Err(error) (SV-R-062) |
| **SV-E-026** | serve_link over a non-self-locating framing receives an undecodable request | Disconnect::Failed, so serve_link returns Err (SV-R-050, SV-R-062); on RTU/ASCII the frame is dropped and serving continues |
| **SV-E-027** | Shutdown races a link failure under serve_link | serve_link's result matches whichever reason on_disconnect received (SV-R-062) |
| **SV-E-041** | serve_link's stream ends between ADUs (e.g. a hung-up tty) | Disconnect::Closed, so serve_link returns Ok(()) (SV-R-052, SV-R-062); on a serial line nobody closes, an Ok(()) not caused by shutdown or a Reject means the device went away |

## Known limitations

- **SV-E-028** — **A corrupt line produces one error per frame, indefinitely.** On RTU or ASCII the server reports every undecodable frame to `on_error` and keeps serving, by design (SV-R-050) — one frame of noise is not allowed to take a device off the bus. A line that is permanently corrupt therefore produces a steady stream of callbacks rather than a closed connection. A service that wants to give up counts them itself, where it knows its own tolerance; the crate cannot pick a threshold that is right for both a quiet bench and a noisy plant floor.
- **SV-E-029** — **No connection limit.** Every accepted connection gets a task, and nothing caps how many. A service that wants a cap enforces it in `on_connect` (SV-R-032), where it has the peer address and its own count — the server cannot choose a number that is right for both an embedded gateway and a SCADA front end.
- **SV-E-030** — **No idle or per-request timeout.** A connection that goes quiet is not closed, and a slow `on_request` is awaited indefinitely. A responder that times out its own handler would answer nothing while the handler still ran, which is worse than being slow; a service that needs a bound applies `tokio::time::timeout` inside `on_request`, where it knows what the operation costs.
- **SV-E-031** — **Requests on one connection are handled one at a time.** Modbus TCP permits an initiator to pipeline transactions; this server reads, dispatches, answers, and only then reads again. Concurrency is per connection (SV-R-030), which matches how initiators behave in practice — including this crate's own client (CL-R-005). Pipelining would need out-of-order responses and its own ordering contract.
- **SV-E-032** — **No data model at all.** See [`data-contract.md`](./data-contract.md). This is the largest deliberate omission in the crate: it means "hello world" for this server is an `impl Service` of a dozen lines, not two.
- **SV-E-033** — **The service is moved into the server, not borrowed.** SV-R-002 takes ownership, and the orphan rule stops a consumer implementing `Service` for `Arc<their type>`. So a consumer that also wants to read its own store keeps its state in `Arc` fields and clones the service. Borrowing instead would tie the server's lifetime to a scope, which a `'static` connection task cannot have.
- **SV-E-034** — **A rejected connection is closed, not refused.** `on_connect` runs after the TCP handshake completed, so a refused peer sees a connection that opens and immediately closes. Refusing before the handshake is not something a listener can express.
- **SV-E-035** — **Shutdown is cooperative, not immediate.** SV-R-044 waits for handlers. A service whose `on_request` never returns keeps `shutdown()` pending forever; the bound belongs to the handler, as above.
- **SV-E-036** — **No built-in accept back-off.** A service answering `Continue` to a persistent error (`EMFILE`) without awaiting inside `on_accept_error` makes the accept loop spin. The delay belongs in the notification, where the service knows its tolerance; a fixed crate-chosen delay would be wrong for some deployment. Whether to retry is read from `error.listener_failure()` (TR-R-101): `Continue` with a delay on `Transient`, `Stop` on `Fatal`.
- **SV-E-040** — **The default `on_receive_error` retries a persistent transient error with no delay.** It answers `Continue` on `Transient` (SV-R-074) without awaiting, so a socket that keeps failing with, e.g., `ENOBUFS` makes the receive loop spin. As with SV-E-036 the delay belongs in the service; one that expects persistent transient errors overrides `on_receive_error` and awaits inside it.
