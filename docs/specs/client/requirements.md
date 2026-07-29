# Client — Requirements

Normative behavior of the async Modbus client (initiator): the public request
API, how requests are issued and responses matched, timeout semantics, retry and
reconnect policy, and how protocol exceptions are surfaced to the caller.

Wire encoding is **not** specified here — it belongs to
[`../frame/`](../frame/). Socket and serial-port behavior belongs to
[`../transport/`](../transport/). This area owns only what is specific to acting
as the initiator.

IDs are stable and append-only (`CL-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (public client
types, methods, configuration fields), [`edge-cases.md`](./edge-cases.md)
(boundary and error behavior, stated limitations).

---

## 1. The client

**CL-R-001** — The client shall be one type, generic over the framing, serving
RTU, ASCII, and TCP alike. Role behavior that differs only by framing shall not
be written three times.

**CL-R-002** — A client shall be constructed from an established transport, not
from an address or a device path. Connecting and opening belong to the transport
area (TR-R-020, TR-R-030), and a client shall not duplicate them.

**CL-R-003** — The client shall address a server by unit identifier on every
request. How that identifier reaches the wire is a property of the framing: the
RTU and ASCII header is the identifier itself (FR-R-096, FR-R-117), the TCP
header carries it beside a transaction identifier (FR-R-101).

**CL-R-004** — The client shall be available only when the `std` feature is
enabled, since it performs I/O and applies timeouts.

**CL-R-005** — The client shall permit at most one request in flight at a time,
and shall enforce this in the type system rather than at run time.

**CL-R-006** — The client shall surrender its transport on request, so that a
caller may reuse the connection or inspect it after a failure.

---

## 2. Issuing a request

**CL-R-010** — Issuing a request shall build the framing's header from the unit
identifier and the transaction identifier, encode the ADU, write it in full, and
then await a response.

**CL-R-011** — The client shall allocate transaction identifiers itself. The
first shall be 1, each subsequent one shall be the previous plus one, and the
sequence shall wrap from 65535 to 1. Zero shall not be allocated, so that a
matched response is never matched against an unset field.

**CL-R-012** — A request that cannot be encoded shall fail without writing any
bytes to the transport.

**CL-R-013** — A partially written request shall not be retried or abandoned
mid-ADU: the write shall be driven to completion or the client shall be
unusable thereafter (CL-R-031). A truncated ADU on the wire desynchronizes the
peer, which no later request can repair.

**CL-R-014** — The response deadline shall start when the request has been
written, not when it was submitted. Time spent writing shall not consume the
time allowed for a reply.

---

## 3. Matching a response

**CL-R-020** — A response shall be accepted only if its header corresponds to
the header sent. For RTU and ASCII the unit identifier shall be equal; for TCP
both the transaction identifier and the unit identifier shall be equal.

**CL-R-021** — A response whose header does not correspond shall be discarded
and the client shall continue waiting. Discarding shall not extend the deadline
of CL-R-014.

**CL-R-022** — A response whose header corresponds but whose function code is
neither the code requested nor an exception to it shall fail immediately with an
unexpected-function error, naming the code expected and the code received.

**CL-R-023** — A response that cannot be decoded shall fail with the frame
area's decoding error unaltered, and shall leave the client unusable (CL-R-031):
a malformed ADU means the byte stream is no longer trusted to be aligned.

**CL-R-024** — A response arriving after its request has timed out shall never
be delivered as the result of a later request. It shall be either discarded by
CL-R-021 or refused by CL-R-031.

---

## 4. Timeouts and desynchronization

**CL-R-030** — The client shall bound the wait for a response by a configurable
response timeout, whose default shall be 1 second.

**CL-R-031** — A response timeout, an I/O failure, or a decoding failure shall
mark the client desynchronized: what the peer will send next is no longer known.

**CL-R-032** — A desynchronized client shall fail every subsequent request
immediately, without writing to the transport, with a distinct error naming the
condition.

**CL-R-033** — Recovery from desynchronization shall be by discarding the client
and establishing a new transport. The client shall not silently resynchronize by
draining, reconnecting, or retrying — each would issue a request the caller did
not authorize.

**CL-R-034** — The client shall report whether it is desynchronized, so that a
caller may discard it without first provoking an error.

---

## 5. Exceptions

**CL-R-040** — A response that is an exception to the function requested shall
be surfaced to a typed request method as a failure carrying both the function
code and the exception code, never as a success value.

**CL-R-041** — An exception code outside those named by the frame area shall be
surfaced unaltered rather than rejected: it is a legal response the server chose
to send (FR-R-072).

**CL-R-042** — Receiving an exception shall leave the client usable. The
exchange completed; the server merely refused the request.

---

## 6. Broadcast

**CL-R-050** — Whether a unit identifier is a broadcast address shall be a
property of the framing: unit 0 shall broadcast on RTU and ASCII (FR-R-096), and
no identifier shall broadcast on TCP.

**CL-R-051** — A write request addressed to a broadcast identifier shall be
written and shall complete without awaiting a response, since no server replies
to a broadcast.

**CL-R-052** — A read request addressed to a broadcast identifier shall fail
before anything is written, since a read whose answer cannot arrive is a caller
error, not a silent no-op.

**CL-R-053** — Issuing a raw request to a broadcast identifier shall succeed
with no response value rather than fail, so the raw path can express a broadcast
that the typed methods forbid.

---

## 7. The request API

**CL-R-060** — The client shall expose one typed method per named function code
of FR-R-010, taking and returning the domain value types of FR-R-007 rather than
bare integers.

**CL-R-061** — The client shall expose a raw method taking a request PDU and
yielding the response PDU as received, including an exception response. This is
how a custom function code (FR-R-011) is issued and how a caller inspects a
response the typed methods interpret.

**CL-R-062** — A typed method reading bits shall return exactly the quantity of
bits requested, discarding the padding bits of the final byte (FR-R-024).

**CL-R-063** — The client shall not validate request field ranges itself. Range
rules are frame-area behavior (FR-R-021, FR-R-027, FR-R-031) and shall surface
from encoding, so one rule has one home.

**CL-R-064** — A typed method shall not compare the fields a server echoes
against the fields sent. An echo mismatch is a server defect the caller may
detect via CL-R-061; the client shall not fail a request over it.
