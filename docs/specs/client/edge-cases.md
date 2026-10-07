# Client — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**. Entries under "Known limitations" are working as implemented; they are recorded here so they are not mistaken for oversights and silently "fixed".

---

## Response handling

| ID | Condition | Behavior |
|---|---|---|
| **CL-E-001** | No response before the deadline | `Timeout { what: "response" }`; the client becomes desynchronized (CL-R-031) |
| **CL-E-002** | Header does not correspond (wrong unit id, or wrong transaction id on TCP) | Discarded, waiting continues against the *original* deadline (CL-R-021, CL-R-097) |
| **CL-E-003** | Header corresponds, function code is another function's | `UnexpectedFunction { expected, actual }`, immediately (CL-R-022) |
| **CL-E-004** | Header corresponds, response is an exception to the function requested | `Exception { function, exception }` from a typed method; the response verbatim from `call` (CL-R-040, CL-R-042) |
| **CL-E-005** | Exception code outside the named set | Surfaced as `ExceptionCode::Other` (CL-R-041) |
| **CL-E-006** | Response undecodable (bad CRC/LRC, bad length, malformed body), TCP | The frame area's error unaltered; the client becomes desynchronized (CL-R-023, CL-R-098) |
| **CL-E-007** | Response undecodable, RTU or ASCII | The frame area's error unaltered; the client stays usable and the next request proceeds (CL-R-023, CL-R-098) |
| **CL-E-008** | A frame split in two by a spurious gap on RTU | Both halves fail their checksum, one per receive; each costs one frame and the link stays usable |
| **CL-E-009** | A late response to a timed-out request arrives during the next request | Discarded by CL-R-021 if it does not correspond — but on RTU/ASCII to the same unit it *does* correspond, which is why CL-R-031 refuses the next request outright |
| **CL-E-010** | Server replies to a broadcast (contrary to the protocol) | The reply is never read by the broadcast request; it is left in the stream. A reply carrying the next request's unit id desynchronizes that exchange; one carrying unit 0 does not correspond and is discarded (CL-R-021) |
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
| **CL-E-028** | Response timeout elapsed | `Unusable(Silent)` — the client stopped waiting; the peer may be alive and slow |
| **CL-E-029** | Response undecodable on TCP | `Unusable(Undecodable)` (CL-R-098) |

- **CL-E-030** — Reading the state touches nothing and blocks on nothing (CL-R-038); it may be called on a client that has never been used and between requests without cost.

## Known limitations

- **CL-E-031** — **A timeout is unrecoverable, not just unanswered.** The transport refuses a receive that follows an abandoned one (TR-R-090), so the client cannot resume by simply retrying. This is stricter than some Modbus clients, which drain and continue; draining cannot distinguish a late reply from the next reply, and guessing wrong returns one server's data as another's.
- **CL-E-032** — **Echoed fields are not verified** (CL-R-064). A server that echoes the wrong address in a code 6 response yields `Ok(())`. Use `call` to inspect the echo.
- **CL-E-033** — **Broadcast writes cannot be confirmed.** CL-R-051 returns as soon as the bytes are written; whether any device acted on them is unobservable by design of the protocol, not of this client.
- **CL-E-034** — **Interop findings are the server's, not the client's.** Verified against an external Modbus TCP server: real servers refuse optional function codes (`IllegalFunction` for code 22) and may perform code 23's read before its write, contrary to the write-then-read order the Modbus Application Protocol specification gives for code 23. The client carries the bytes and surfaces what came back; it does not normalise either behavior away.
- **CL-E-035** — **No unit-id default.** Every method names its unit explicitly (CL-R-003). A default would make the most consequential argument of a Modbus request the one most easily forgotten.
- **CL-E-036** — **`Answered` is history, not liveness.** It says a peer replied to the last request this client sent, not that one would reply now. On TCP a peer that vanished without a FIN is indistinguishable from an idle one until bytes are written, so no local check can do better. A failover built on `Answered` meaning "alive" is built on a guarantee that does not exist.
- **CL-E-037** — **A quiet link and a dead one are the same observation.** `Silent` means this client's deadline elapsed. A server that is merely slow, a cable that was pulled, and a process that was killed all produce it, and the client does not guess between them — which is why it is not folded into `PeerClosed`.
- **CL-E-038** — **There is no probe** (CL-R-039). Proving a peer answers costs a request, and unauthorized requests are what CL-R-033 exists to prevent. A caller that wants one issues it with `call` and applies its own policy to the result.
- **CL-E-039** — **A reason is a promise.** Both reported enums are exhaustive (NF-R-017), so a fifth reason is a breaking change. The four are coarse on purpose; finer classification would either need `#[non_exhaustive]` — which this crate does not use — or a minor bump every time the platform surprises us.

---

## The blocking client

| ID | Condition | Behavior |
|---|---|---|
| **CL-E-040** | A blocking method called from inside a runtime | `BlockingInAsyncContext`, before anything is written (CL-R-075) |
| **CL-E-041** | Two blocking calls back to back with no sleep | Both succeed; the runtime is driven to completion inside each call (CL-R-077) |
| **CL-E-042** | Response timeout on a blocking call | Identical to async: `Timeout { what: "response" }`, client desynchronized (CL-R-030, CL-R-031) |
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
