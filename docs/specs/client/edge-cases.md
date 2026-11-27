# Client — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**. Entries under "Known limitations" are working as implemented; they are recorded here so they are not mistaken for oversights and silently "fixed".

---

## Response handling

| ID | Condition | Behavior |
|---|---|---|
| **CL-E-001** | No response before the deadline | `Timeout { what: "response" }`; the client becomes desynchronized (CL-R-031), except over a discarding transport, where it stays usable and reports `Unanswered` (CL-R-116) |
| **CL-E-002** | Header does not correspond (wrong unit id, or wrong transaction id on TCP) | Discarded, waiting continues against the *original* deadline (CL-R-021, CL-R-097) |
| **CL-E-003** | Header corresponds, function code is another function's | `UnexpectedFunction { expected, actual }`, immediately (CL-R-022); over a discarding transport, discarded and waiting continues against the original deadline (CL-R-117) |
| **CL-E-004** | Header corresponds, response is an exception to the function requested | `Exception { function, exception }` from a typed method; the response verbatim from `call` (CL-R-040, CL-R-042) |
| **CL-E-005** | Exception code outside the named set | Surfaced as `ExceptionCode::Other` (CL-R-041) |
| **CL-E-006** | Response undecodable (bad CRC/LRC, bad length, malformed body), TCP | The frame area's error unaltered; the client becomes desynchronized (CL-R-023, CL-R-098) |
| **CL-E-007** | Response undecodable, RTU or ASCII | The frame area's error unaltered; the client stays usable and the next request proceeds (CL-R-023, CL-R-098) |
| **CL-E-008** | A frame split in two by a spurious gap on RTU | Both halves fail their checksum, one per receive; each costs one frame and the link stays usable |
| **CL-E-009** | A late response to a timed-out request arrives during the next request | Discarded by CL-R-021 if it does not correspond — but on RTU/ASCII to the same unit it *does* correspond, which is why CL-R-031 refuses the next request outright. Over a discarding transport the next request proceeds: a late response already received is removed by the discard before the write (TR-R-112); one arriving after it is discarded if its function code or length does not fit (CL-R-117, CL-R-118), otherwise delivered (CL-E-054) |
| **CL-E-010** | Server replies to a broadcast (contrary to the protocol) | The reply is never read by the broadcast request; it is left in the stream. A reply carrying the next request's unit id desynchronizes that exchange; one carrying unit 0 does not correspond and is discarded (CL-R-021). Over a discarding transport a reply received before the next request is written is removed by its discard (TR-R-112) |
| **CL-E-011** | `PipelinedClient`/`PipelinedUdpClient`: response transaction id belongs to a request already resolved by timeout or desync | Discarded, no effect on any other in-flight request (CL-R-088) |
| **CL-E-012** | `PipelinedClient`/`PipelinedUdpClient`: response transaction id was never issued | Connection desynchronized, on both transports (CL-R-089) |

- **CL-E-013** — The deadline is absolute and fixed when the write completes (CL-R-014, CL-R-097). A stream of mismatched responses therefore cannot hold a request open indefinitely.

## Connection loss

| ID | Condition | Behavior |
|---|---|---|
| **CL-E-014** | Peer closes cleanly before any response byte | `Io { kind: UnexpectedEof }` from the transport (TR-R-014); desynchronized |
| **CL-E-015** | Peer closes mid-ADU | `ConnectionClosed`; desynchronized |
| **CL-E-016** | Write fails mid-ADU | The I/O error, unaltered; desynchronized — a truncated ADU is on the wire (CL-R-013) |
| **CL-E-017** | Any request on a desynchronized client | `Desynchronized`, with nothing written (CL-R-032) |
| **CL-E-018** | A typed read addressed to unit 0 on RTU/ASCII | `IllegalValue { field: "broadcast read", value: 0 }`, with nothing written (CL-R-052) |

- **CL-E-019** — There is **no reconnect and no retry** (CL-R-033). Recovery is `into_inner`, a new transport, and a new client. This is deliberate: a retried write is a second request the caller never authorized, and on a serial line a duplicated write is observable at the device.

## What the reported state says

| ID | Condition | Reported state |
|---|---|---|
| **CL-E-020** | Client just constructed | `Untried` (CL-R-035) |
| **CL-E-021** | Only broadcast writes issued so far | `Untried` — the bytes went out, but no peer answered and none was expected (CL-R-102) |
| **CL-E-022** | Last exchange returned a response, an `Exception`, or `UnexpectedFunction` | `Answered` — the peer replied, whatever it said (CL-R-036) |
| **CL-E-023** | Last response failed to decode on RTU or ASCII | `Unanswered`; the client stays usable and the next request proceeds (CL-R-098) |
| **CL-E-024** | Broadcast write after an answered exchange | `Answered`, unchanged — a broadcast neither confirms nor refutes what came before (CL-R-102) |
| **CL-E-025** | Peer closed cleanly before any response byte, or mid-ADU | `Unusable(PeerClosed)` (CL-R-037) |
| **CL-E-026** | Write failed because the peer had gone | `Unusable(Io { kind })` — typically `BrokenPipe` or `ConnectionReset`; only an end of stream on the *read* side is classified `PeerClosed` |
| **CL-E-027** | Any other I/O failure, either direction | `Unusable(Io { kind })` (CL-R-037) |
| **CL-E-028** | Response timeout elapsed | `Unusable(Silent)` — the client stopped waiting; the peer may be alive and slow. Over a discarding transport, `Unanswered` and the client stays usable (CL-R-116) |
| **CL-E-029** | Response undecodable on TCP | `Unusable(Undecodable)` (CL-R-098) |

- **CL-E-030** — Reading the state touches nothing and blocks on nothing (CL-R-038); it may be called on a client that has never been used and between requests without cost.

## Known limitations

- **CL-E-031** — **A timeout is unrecoverable, not just unanswered — except over a discarding transport.** The transport refuses a receive that follows an abandoned one (TR-R-090), so the client cannot resume by simply retrying. This is stricter than some Modbus clients, which drain and continue; draining cannot distinguish a late reply from the next reply, and guessing wrong returns one server's data as another's. A discarding transport (TR-R-111) — RTU or ASCII over a stream that can discard its input — stays usable (CL-R-116): the framing is self-locating, the discard before each request writes nothing and so issues no request the caller did not authorize (CL-R-033), and replies that do not fit are dropped (CL-R-117, CL-R-118). The guess is narrowed, not removed (CL-E-054).
- **CL-E-032** — **Echoed fields are not verified** (CL-R-064). A server that echoes the wrong address in a code 6 response yields `Ok(())`. Use `call` to inspect the echo.
- **CL-E-033** — **Broadcast writes cannot be confirmed.** CL-R-051 returns as soon as the bytes are written; whether any device acted on them is unobservable by design of the protocol, not of this client.
- **CL-E-034** — **Interop findings are the server's, not the client's.** Verified against an external Modbus TCP server: real servers refuse optional function codes (`IllegalFunction` for code 22) and may perform code 23's read before its write, contrary to the write-then-read order the Modbus Application Protocol specification gives for code 23. The client carries the bytes and surfaces what came back; it does not normalise either behavior away.
- **CL-E-035** — **No unit-id default.** Every method names its unit explicitly (CL-R-003). A default would make the most consequential argument of a Modbus request the one most easily forgotten.
- **CL-E-036** — **`Answered` is history, not liveness.** It says a peer replied to the last request this client sent, not that one would reply now. On TCP a peer that vanished without a FIN is indistinguishable from an idle one until bytes are written, so no local check can do better. A failover built on `Answered` meaning "alive" is built on a guarantee that does not exist.
- **CL-E-037** — **A quiet link and a dead one are the same observation.** `Silent` means this client's deadline elapsed. A server that is merely slow, a cable that was pulled, and a process that was killed all produce it, and the client does not guess between them — which is why it is not folded into `PeerClosed`.
- **CL-E-038** — **There is no probe** (CL-R-039). Proving a peer answers costs a request, and unauthorized requests are what CL-R-033 exists to prevent. A caller that wants one issues it with `call` and applies its own policy to the result.
- **CL-E-039** — **A reason is a promise.** Both reported enums are exhaustive (NF-R-017), so a fifth reason is a breaking change. The four are coarse on purpose; finer classification would either need `#[non_exhaustive]` — which this crate does not use — or a minor bump every time the platform surprises us.
- **CL-E-050** — **A zero per-call timeout is not rejected.** `call_with_timeout(.., Duration::ZERO)` writes the request in full and then fails with `Timeout { what: "response" }`, leaving the client `Unusable(Silent)`, or `Unanswered` over a discarding transport (CL-R-112, CL-R-116). On a pipelined handle, a zero passed to `send_with_timeout`/`try_send_with_timeout` writes the request and then has the consequences of CL-R-115.
- **CL-E-051** — **A broadcast ignores the per-call timeout.** `call_with_timeout` addressed to a broadcast unit writes the request PDU, read or write alike, and returns `Ok(None)` once the write completes, whatever the timeout, as through `call` (CL-R-053, CL-R-112).
- **CL-E-052** — **Typed methods take no per-call timeout.** Only the raw method has a per-call variant (CL-R-110). A caller that needs a typed request with its own timeout builds the `RequestPdu`, issues it through `call_with_timeout`, and interprets the response itself.
- **CL-E-053** — **A response timeout too large to form a deadline means no deadline.** When a response timeout, configured (CL-R-030, CL-R-093) or passed per call (CL-R-110, CL-R-113), added to the moment the deadline starts, cannot be represented as an instant, the wait has no bound. It does not panic and does not fail.
- **CL-E-054** — **A late reply that fits is indistinguishable from the answer.** Over a discarding transport, a reply to a timed-out request that arrives after the next request's discard (TR-R-112) and matches that request in unit, function code and length (CL-R-020, CL-R-117, CL-R-118) is delivered as its result (CL-R-024). RTU and ASCII carry no transaction id; nothing on the wire separates the two. A caller whose device answers later than the response timeout raises the timeout.
- **CL-E-055** — **Discarding is chosen at construction, not detected.** A `FrameTransport` built by `new` or `with_config` never discards, even over `SerialStream` (TR-R-109), so a client over it keeps the rules of CL-R-022 and CL-R-031. `open_serial` and the blocking serial constructors build a discarding one (TR-R-110); `Client::into_inner` (CL-R-006) returns the transport with its capability intact, but rebuilding from `FrameTransport::into_inner`'s stream with `new` drops it.
- **CL-E-056** — **Not every response length is checked.** Function codes 8, 12, 17, 20, 21, 24 and 43, and every custom code, have no request-determined response length (CL-R-119); over a discarding transport a reply to them with the requested function code is accepted at whatever length decodes (CL-R-118).
- **CL-E-057** — **An undecodable frame still ends the request.** Over a discarding transport, a frame that fails to decode (bad CRC/LRC) fails the request with the frame area's error rather than being dropped (CL-R-023, CL-R-099); the client stays usable (CL-R-098). A late reply split by the discard (TR-R-112) — its head discarded, its tail received — produces exactly this.

---

## The blocking client

| ID | Condition | Behavior |
|---|---|---|
| **CL-E-040** | A blocking method called from inside a runtime | `BlockingInAsyncContext`, before anything is written (CL-R-075) |
| **CL-E-041** | Two blocking calls back to back with no sleep | Both succeed; the runtime is driven to completion inside each call (CL-R-077) |
| **CL-E-042** | Response timeout on a blocking call | Identical to async: `Timeout { what: "response" }`, client desynchronized (CL-R-030, CL-R-031); `SyncRtuClient`/`SyncAsciiClient` open through `open_serial` (TR-R-110) and stay usable (CL-R-116) |
| **CL-E-043** | A blocking call on a desynchronized client | Refused immediately, nothing written (CL-R-032) |
| **CL-E-044** | Broadcast write / broadcast read, blocking | Identical to async (CL-R-051, CL-R-052) |

### Known limitations

- **CL-E-045** — **No `into_inner`.** See `api-contract.md` `## The blocking client` — the transport is only useful to a caller that has a runtime, and such a caller is served by `Client`.
- **CL-E-046** — **One runtime per client.** Two blocking clients own two runtimes and two threads' worth of drivers. A caller creating many is better served by the async client on one runtime.
- **CL-E-047** — **No blocking server** (CL-R-079).

---

## Pipelined clients

`PipelinedClient` (TCP) and `PipelinedUdpClient` (UDP) share one engine and diverge on exactly one rule: what a single request's timeout means for the other requests sharing the connection.

| ID | Condition | `PipelinedClient` (TCP) | `PipelinedUdpClient` (UDP) |
|---|---|---|---|
| **CL-E-048** | One request times out | Desynchronizes the whole connection; every other in-flight request fails immediately (CL-R-090) | Fails only that request; the connection and other in-flight requests are unaffected (CL-R-091) |
| **CL-E-049** | I/O failure | Desynchronizes the whole connection (CL-R-031, CL-R-109) | Desynchronizes the whole connection (CL-R-031, CL-R-109) |

The two disagree here because a timeout means something different per transport. TCP is a stream: a socket that produces sporadic long delays is itself suspect, in the same way an I/O failure or EOF is — CL-R-090 keeps the conservative posture CL-R-031/032 already apply to the single-request client. UDP is packetized and unreliable by design: one dropped or slow datagram is routine and says nothing about whether the socket can still send and receive the next one, so treating it as connection-wide would make ordinary packet loss disproportionately expensive to a caller. A response carrying a transaction id neither type ever issued (CL-R-089) is not subject to this split — it signals a genuine correctness violation on either transport, not network unreliability, so both desynchronize.
