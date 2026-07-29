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
| Response undecodable (bad CRC/LRC, bad length, malformed body) | The frame area's error unaltered; the client becomes desynchronized (CL-R-023) |
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

There is **no reconnect and no retry** (CL-R-033). Recovery is `into_inner`, a
new transport, and a new client. This is deliberate: a retried write is a second
request the caller never authorized, and on a serial line a duplicated write is
observable at the device.

## 3. Known limitations

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
- **No unit-id default.** Every method names its unit explicitly (CL-R-003). A
  default would make the most consequential argument of a Modbus request the one
  most easily forgotten.
