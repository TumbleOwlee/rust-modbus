# Server — Requirements

Normative behavior of the async Modbus server (responder): serving a listener or
a single link, per-connection handling, dispatching decoded requests to the
consumer's service, unit-identifier filtering, exception generation, connection
lifecycle notifications, and shutdown.

Wire encoding is **not** specified here — it belongs to
[`../frame/`](../frame/). Socket and serial-port behavior belongs to
[`../transport/`](../transport/). This area owns only what is specific to acting
as the responder.

IDs are stable and append-only (`SV-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (public server
types, the service trait, configuration fields),
[`data-contract.md`](./data-contract.md) (why this area owns no data model),
[`edge-cases.md`](./edge-cases.md) (boundary and error behavior, stated
limitations).

---

## 1. The server

**SV-R-001** — The server shall be one type, generic over the framing and over
the consumer's service, serving RTU, ASCII, and TCP alike. Role behavior that
differs only by framing shall not be written three times.

**SV-R-002** — A server shall be constructed from a service value, shall take
ownership of it, and shall share that one value across every connection it
handles.

**SV-R-003** — Request handling shall be defined by a trait whose methods take a
shared reference to the service, so that requests on distinct connections may be
handled concurrently. The trait shall require that the service be safe to share
between threads and shall place no synchronization of its own around it: any
mutable state and its locking belong to the implementor.

**SV-R-004** — The trait shall require only request handling. Its connection
lifecycle notifications shall have default behavior — accept the connection,
ignore the notification — so that a minimal service implements one method.

**SV-R-005** — The crate shall supply no implementation of the trait and no data
model of coils, discrete inputs, or registers. What a request means is the
implementor's, not this crate's.

**SV-R-006** — The server shall be available only when the `std` feature is
enabled, since it performs I/O and spawns tasks.

**SV-R-007** — The server shall serve either a TCP listener, accepting many
connections and handling them concurrently, or a single already-established
transport such as a serial link. Both shall run the same per-connection
behavior.

**SV-R-008** — The server shall accept requests addressed to any unit identifier
unless configured otherwise (SV-R-020), and shall have no other configuration
whose default is not stated in
[`api-contract.md`](./api-contract.md).

---

## 2. The exchange

**SV-R-010** — For each request received on a connection the server shall
dispatch the decoded request PDU, together with the unit identifier it was
addressed to and the identity of the connection, to the service, and shall send
the service's answer on that same connection.

**SV-R-011** — A response shall carry the header of the request it answers, so
that an initiator's matching rule (CL-R-020) succeeds: on TCP both the
transaction identifier and the unit identifier shall be those received, and on
RTU and ASCII the unit identifier shall be the one received.

**SV-R-012** — A service that refuses a request shall do so by naming an
exception code, and the server shall send that refusal as an exception response
to the function code of the request received (FR-R-070).

**SV-R-013** — A response the service returns shall be sent unaltered. The
server shall not substitute, validate, or reinterpret it: a service that answers
one function code with another's response has said what it meant to say.

**SV-R-014** — A response that cannot be encoded shall be reported as a
per-request error (SV-R-050) and shall not end the connection. Nothing was
written, so the stream remains aligned.

**SV-R-015** — The server shall handle successive requests on one connection
until the peer closes it, a failure ends it (SV-R-051), or shutdown is requested
(SV-R-041).

---

## 3. Unit identifiers

**SV-R-020** — The server shall optionally be configured with a unit identifier.
When one is configured, only requests addressed to that identifier shall be
dispatched to the service.

**SV-R-021** — A request whose unit identifier does not match the configured one
shall draw no response at all and shall not end the connection: on a shared
serial line the request belongs to another device, and answering it would corrupt
that exchange.

**SV-R-022** — When no unit identifier is configured, every request shall be
dispatched regardless of the identifier it carries, and the service shall decide
whether to answer or to refuse.

**SV-R-023** — A request addressed to a broadcast identifier (FR-R-096) shall be
dispatched to the service and shall never be answered, whatever the
configuration.

---

## 4. Connections

**SV-R-030** — Each connection accepted from a listener shall be handled
independently of the others, so that a request in flight on one connection does
not delay a request on another.

**SV-R-031** — Each connection shall be given an identity comprising an
identifier unique within the server's lifetime, assigned in the order
connections are taken up, and the peer's address where the transport has one.

**SV-R-032** — The service shall be notified of a new connection before any
request is read from it, and may refuse it. A refused connection shall be closed
without reading a request. The answer shall be a named choice, not a boolean, so
that neither the implementor nor the reader must remember which way `true` points.

**SV-R-033** — The service shall be notified exactly once of the end of every
connection it was notified of, and that notification shall name why the
connection ended: the peer closed it, the service refused it, a failure ended
it, or the server is shutting down.

**SV-R-034** — The service shall be notified of every per-request failure. That
notification shall not itself end the connection; whether the connection
continues is decided by the failure (SV-R-014, SV-R-050).

**SV-R-035** — A failure on one connection shall neither affect another
connection nor stop the server accepting new ones.

**SV-R-036** — Every notification concerning a connection shall carry that
connection's identity, so that a service may key its own state by connection.

---

## 5. Shutdown

**SV-R-040** — The server shall provide a shutdown handle, obtainable before
serving begins, by which shutdown is requested explicitly.

**SV-R-041** — Once shutdown is requested, the server shall accept no further
connection and read no further request.

**SV-R-042** — A request already dispatched when shutdown is requested shall run
to completion and its response shall be sent before its connection closes.

**SV-R-043** — Each connection still live when shutdown is requested shall end
with the shutting-down reason of SV-R-033.

**SV-R-044** — A shutdown request shall complete only once every connection has
finished and serving has returned, so that a caller awaiting it knows no
handler is still running.

**SV-R-045** — The handle shall report whether shutdown has been requested,
without requesting it.

---

## 6. Errors

**SV-R-050** — A request that cannot be decoded shall be reported to the service
(SV-R-034). It shall end the connection only where the framing is not
self-locating (FR-R-144); on a self-locating framing the failure shall cost
exactly that frame and serving shall continue with the next request. No response
shall be sent for a request that could not be decoded, on either framing. This is
the responder's counterpart to CL-R-023.

**SV-R-051** — A failure confined to one connection shall not propagate out of
serving. Serving shall fail only for a failure of the listener itself.

**SV-R-052** — A peer that closes the connection between two ADUs shall end the
connection with the closed reason of SV-R-033, not as a failure. A close
part-way through an ADU is a failure (TR-R-014).

**SV-R-053** — Serving a TCP listener shall be available for any framing, so that
a listener accepting gateway-framed connections runs the same per-connection
behavior as one accepting MBAP-framed connections (SV-R-007).
