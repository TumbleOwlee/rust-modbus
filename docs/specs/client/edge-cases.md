# Client — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**.
Entries under "Known limitations" are working as implemented; they are recorded
here so they are not mistaken for oversights and silently "fixed".

---

## 1. Response handling

| Condition | Behavior |
|---|---|
| No response before the deadline | `Timeout { what: "response" }`; the client becomes desynchronized (CL-R-031) |
| Header does not correspond (wrong unit id, or wrong transaction id on TCP) | Discarded, waiting continues against the *original* deadline (CL-R-021) |
| Header corresponds, function code is another function's | `UnexpectedFunction { expected, actual }`, immediately (CL-R-022) |
| Header corresponds, response is an exception to the function requested | `Exception { function, exception }` from a typed method; the response verbatim from `call` (CL-R-040, CL-R-042) |
| Exception code outside the named set | Surfaced as `ExceptionCode::Other` (CL-R-041) |
| Response undecodable (bad CRC/LRC, bad length, malformed body), TCP | The frame area's error unaltered; the client becomes desynchronized (CL-R-023) |
| Response undecodable, RTU or ASCII | The frame area's error unaltered; the client stays usable and the next request proceeds (CL-R-023) |
| A frame split in two by a spurious gap on RTU | Both halves fail their checksum, one per receive; each costs one frame and the link stays usable |
| A late response to a timed-out request arrives during the next request | Discarded by CL-R-021 if it does not correspond — but on RTU/ASCII to the same unit it *does* correspond, which is why CL-R-031 refuses the next request outright |
| Server replies to a broadcast (it should not) | The reply is never read by the broadcast request; it is left in the stream and desynchronizes the next exchange |

The deadline is absolute and fixed when the write completes (CL-R-014). A stream
of mismatched responses therefore cannot hold a request open indefinitely.

## 2. Connection loss

| Condition | Behavior |
|---|---|
| Peer closes cleanly before any response byte | `Io { kind: UnexpectedEof }` from the transport (TR-R-014); desynchronized |
| Peer closes mid-ADU | `ConnectionClosed`; desynchronized |
| Write fails mid-ADU | The I/O error, unaltered; desynchronized — a truncated ADU is on the wire (CL-R-013) |
| Any request on a desynchronized client | `Desynchronized`, with nothing written (CL-R-032) |
| A typed read addressed to unit 0 on RTU/ASCII | `IllegalValue { field: "broadcast read", value: 0 }`, with nothing written (CL-R-052) |

There is **no reconnect and no retry** (CL-R-033). Recovery is `into_inner`, a
new transport, and a new client. This is deliberate: a retried write is a second
request the caller never authorized, and on a serial line a duplicated write is
observable at the device.

## 3. What the reported state says

| Condition | Reported state |
|---|---|
| Client just constructed | `Untried` (CL-R-035) |
| Only broadcast writes issued so far | `Untried` — the bytes went out, but no peer answered and none was expected (CL-R-036) |
| Last exchange returned a response, an `Exception`, or `UnexpectedFunction` | `Answered` — the peer replied, whatever it said (CL-R-036) |
| Last response failed to decode on RTU or ASCII | `Unanswered`; the client stays usable and the next request proceeds (CL-R-023) |
| Broadcast write after an answered exchange | `Answered`, unchanged — a broadcast neither confirms nor refutes what came before (CL-R-036) |
| Peer closed cleanly before any response byte, or mid-ADU | `Unusable(PeerClosed)` (CL-R-037) |
| Write failed because the peer had gone | `Unusable(Io { kind })` — typically `BrokenPipe` or `ConnectionReset`; only an end of stream on the *read* side is classified `PeerClosed` |
| Any other I/O failure, either direction | `Unusable(Io { kind })` (CL-R-037) |
| Response timeout elapsed | `Unusable(Silent)` — the client stopped waiting; the peer may be alive and slow |
| Response undecodable on TCP | `Unusable(Undecodable)` (CL-R-023) |

Reading the state touches nothing and blocks on nothing (CL-R-038); it may be
called on a client that has never been used and between requests without cost.

## 4. Known limitations

- **A timeout is unrecoverable, not just unanswered.** The transport refuses a
  receive that follows an abandoned one (TR-R-041), so the client cannot resume
  by simply retrying. This is stricter than some Modbus clients, which drain and
  continue; draining cannot distinguish a late reply from the next reply, and
  guessing wrong returns one server's data as another's.
- **One request in flight, no pipelining.** Modbus TCP permits several
  outstanding transactions distinguished by transaction identifier. `&mut self`
  forbids it (CL-R-005). Pipelining is a product decision with its own
  matching, ordering, and cancellation semantics, not a mechanical extension.
- **Echoed fields are not verified** (CL-R-064). A server that echoes the wrong
  address in a code 6 response yields `Ok(())`. Use `call` to inspect the echo.
- **Broadcast writes cannot be confirmed.** CL-R-051 returns as soon as the
  bytes are written; whether any device acted on them is unobservable by design
  of the protocol, not of this client.
- **Interop findings are the server's, not the client's.** Verified against an
  external Modbus TCP server: real servers refuse optional function codes
  (`IllegalFunction` for code 22) and may perform code 23's read before its
  write, contrary to §6.17. The client carries the bytes and surfaces what came
  back; it does not normalise either behavior away.
- **No unit-id default.** Every method names its unit explicitly (CL-R-003). A
  default would make the most consequential argument of a Modbus request the one
  most easily forgotten.
- **`Answered` is history, not liveness.** It says a peer replied to the last
  request this client sent, not that one would reply now. On TCP a peer that
  vanished without a FIN is indistinguishable from an idle one until bytes are
  written, so no local check can do better. A failover built on `Answered`
  meaning "alive" is built on a guarantee that does not exist.
- **A quiet link and a dead one are the same observation.** `Silent` means this
  client's deadline elapsed. A server that is merely slow, a cable that was
  pulled, and a process that was killed all produce it, and the client does not
  guess between them — which is why it is not folded into `PeerClosed`.
- **There is no probe** (CL-R-039). Proving a peer answers costs a request, and
  unauthorized requests are what CL-R-033 exists to prevent. A caller that wants
  one issues it with `call` and applies its own policy to the result.
- **A reason is a promise.** Both reported enums are exhaustive (NF-R-017), so a
  fifth reason is a breaking change. The four are coarse on purpose; finer
  classification would either need `#[non_exhaustive]` — which this crate does
  not use — or a minor bump every time the platform surprises us.

---

## 5. The blocking client

| Condition | Behavior |
|---|---|
| A blocking method called from inside a runtime | `BlockingInAsyncContext`, before anything is written (CL-R-075) |
| Two blocking calls back to back with no sleep | Both succeed; the runtime is driven to completion inside each call (CL-R-077) |
| Response timeout on a blocking call | Identical to async: `Timeout { what: "response" }`, client desynchronized (CL-R-030, CL-R-031) |
| A blocking call on a desynchronized client | Refused immediately, nothing written (CL-R-032) |
| Broadcast write / broadcast read, blocking | Identical to async (CL-R-051, CL-R-052) |

### Known limitations

- **No `into_inner`.** See `api-contract.md` §7 — the transport is only useful to
  a caller that has a runtime, and such a caller should use `Client`.
- **One runtime per client.** Two blocking clients own two runtimes and two
  threads' worth of drivers. A caller creating many should use the async client
  on one runtime instead.
- **No blocking server** (CL-R-079).
