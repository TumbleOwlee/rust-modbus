# Server — Requirements

Normative behavior of the async Modbus server (responder): serving a listener or a single link, per-connection handling, dispatching decoded requests to the consumer's service, unit-identifier filtering, exception generation, connection lifecycle notifications, and shutdown.

Wire encoding is **not** specified here — it belongs to [`../frame/`](../frame/). Socket and serial-port behavior belongs to [`../transport/`](../transport/). This area owns only what is specific to acting as the responder.

IDs are stable and append-only (`SV-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (public server types, the service trait, configuration fields), [`data-contract.md`](./data-contract.md) (why this area owns no data model), [`edge-cases.md`](./edge-cases.md) (boundary and error behavior, stated limitations).

---

## The server

**SV-R-001** — The server is one type, generic over the framing and over the consumer's service, serving RTU, ASCII, and TCP alike. Role behavior that differs only by framing is not written three times.

**SV-R-002** — A server is constructed from a service value, takes ownership of it, and shares that one value across every connection it handles.

**SV-R-003** — Request handling is defined by a trait whose methods take a shared reference to the service, so that requests on distinct connections may be handled concurrently. The trait requires that the service be safe to share between threads and places no synchronization of its own around it: any mutable state and its locking belong to the implementor.

**SV-R-004** — The trait requires only request handling. Its connection lifecycle notifications have default behavior — accept the connection, ignore the notification — so that a minimal service implements one method.

**SV-R-005** — The crate supplies no implementation of the trait and no data model of coils, discrete inputs, or registers. What a request means is the implementor's, not this crate's.

**SV-R-006** — The server is available only when the `std` feature is enabled, since it performs I/O and spawns tasks.

**SV-R-007** — The server serves either a TCP listener, accepting many connections and handling them concurrently, or a single already-established transport such as a serial link. Both run the same per-connection behavior.

**SV-R-008** — The server accepts requests addressed to any unit identifier unless configured otherwise (SV-R-020).

**SV-R-063** — The server has no configuration whose default is not stated in [`api-contract.md`](./api-contract.md).

**SV-R-069** — The server area's API is identical with and without the `rtu` feature; serial transports come from the transport area (TR-R-032).

**SV-R-070** — The server never binds a socket: every serving entry point takes a listener, socket or link the caller has already opened, so the bound address is read from the caller's own handle, never from the server.

---

## The exchange

**SV-R-010** — For each request received on a connection the server dispatches the decoded request PDU, together with the unit identifier it was addressed to and the identity of the connection, to the service, and sends the service's answer on that same connection, when it returns one (SV-R-024).

**SV-R-011** — A response carries the header of the request it answers, so that an initiator's matching rule (CL-R-020) succeeds: on TCP both the transaction identifier and the unit identifier are those received, and on RTU and ASCII the unit identifier is the one received.

**SV-R-012** — A service that refuses a request does so by naming an exception code, and the server sends that refusal as an exception response to the function code of the request received (FR-R-070).

**SV-R-013** — A response the service returns is sent unaltered. The server does not substitute, validate, or reinterpret it: a service that answers one function code with another's response has said what it meant to say.

**SV-R-014** — A response that cannot be encoded is reported as a per-request error (SV-R-050) and does not end the connection. Nothing was written, so the stream remains aligned.

**SV-R-015** — The server handles successive requests on one connection until the peer closes it, a failure ends it (SV-R-051), or shutdown is requested (SV-R-041).

---

## Unit identifiers

**SV-R-020** — The server is optionally configured with a unit identifier. When one is configured, only requests addressed to that identifier are dispatched to the service.

**SV-R-021** — A request whose unit identifier does not match the configured one draws no response at all and does not end the connection: on a shared serial line the request belongs to another device, and answering it would corrupt that exchange.

**SV-R-022** — When no unit identifier is configured, every request is dispatched regardless of the identifier it carries, and the service decides whether to answer or to refuse.

**SV-R-023** — A request addressed to a broadcast identifier (FR-R-096) is dispatched to the service and never answered, whatever the configuration.

**SV-R-024** — `Service::on_request` returns `Result<Option<ResponsePdu>, ExceptionCode>`. `Ok(Some(response))` is sent per SV-R-010 and SV-R-013. `Ok(None)` sends no response and does not end the connection, exactly as for a broadcast request (SV-R-023) or a non-matching unit id (SV-R-021) — the service withholding its own answer rather than the wire deciding it. `Err(exception)` still draws an exception response (SV-R-012), unaffected.

---

## Connections

**SV-R-030** — Each connection accepted from a listener is handled independently of the others, so that a request in flight on one connection does not delay a request on another.

**SV-R-031** — Each connection is given an identity comprising an identifier unique within the server's lifetime, assigned in the order connections are taken up, and the peer's address where the transport has one.

**SV-R-032** — The service is notified of a new connection before any request is read from it, and may refuse it. A refused connection is closed without reading a request.

**SV-R-064** — The service's answer to a new-connection notification (SV-R-032) is a named choice, not a boolean, so that neither the implementor nor the reader has to remember which way `true` points.

**SV-R-033** — The service is notified exactly once of the end of every connection it was notified of, and that notification names why the connection ended: the peer closed it, the service refused it, a failure ended it, or the server is shutting down.

**SV-R-034** — The service is notified of every per-request failure. That notification does not itself end the connection; whether the connection continues is decided by the failure (SV-R-014, SV-R-050).

**SV-R-035** — A failure on one connection neither affects another connection nor stops the server accepting new ones.

**SV-R-036** — Every notification concerning a connection carries that connection's identity, so that a service may key its own state by connection.

**SV-R-055** — `Connection` exposes the verified client certificate presented during a TLS handshake: `Some` on a TLS connection under `ClientCertPolicy::Require` that accepted one, `None` on plain TCP, RTU/ASCII, or a TLS connection under `ClientCertPolicy::None`.

**SV-R-056** — Behind the `tls` feature, `Service` provides `on_tls_handshake_failed`, notified when a TLS handshake fails before any `Connection` is established, taking the peer's socket address and the error that failed the handshake. It has default behavior of ignoring the notification, so an existing implementor is unaffected. No `Connection`/ `ConnectionId` is ever assigned to a connection whose handshake failed, since the peer was never accepted as a connection (SV-R-031).

**SV-R-071** — `Connection` implements `Clone` and not `Copy`, in every feature combination.

---

## Shutdown

**SV-R-040** — The server provides a shutdown handle, obtainable before serving begins, by which shutdown is requested explicitly.

**SV-R-041** — Once shutdown is requested, the server accepts no further connection and reads no further request.

**SV-R-042** — A request already dispatched when shutdown is requested runs to completion and its response is sent before its connection closes.

**SV-R-043** — Each connection still live when shutdown is requested ends with the shutting-down reason of SV-R-033.

**SV-R-044** — A shutdown request completes only once every connection has finished and serving has returned, so that a caller awaiting it knows no handler is still running.

**SV-R-045** — The handle reports whether shutdown has been requested, without requesting it.

---

## Errors

**SV-R-050** — A request that cannot be decoded is reported to the service (SV-R-034). It ends the connection only where the framing is not self-locating (FR-R-144); on a self-locating framing the failure costs exactly that frame and serving continues with the next request. This is the responder's counterpart to CL-R-098.

**SV-R-065** — No response is sent for a request that could not be decoded (SV-R-050), on either framing.

**SV-R-051** — A failure confined to one connection does not propagate out of serving a listener. Serving a listener fails only for a failure of the listener itself that the service answers with `AcceptErrorAction::Stop` (SV-R-059, SV-R-067). Serving a UDP socket fails only for a failure receiving from the socket that the service answers with `AcceptErrorAction::Stop` (SV-R-073, SV-R-078). Serving a single link returns per SV-R-062.

**SV-R-052** — A peer that closes the connection between two ADUs ends the connection with the closed reason of SV-R-033, not as a failure. A close part-way through an ADU is a failure (TR-R-088).

**SV-R-053** — Serving a TCP listener is available for any framing, so that a listener accepting gateway-framed connections runs the same per-connection behavior as one accepting MBAP-framed connections (SV-R-007).

**SV-R-054** — Behind the crate's `serde` feature, `ServerConfig` implements `serde::Serialize` and `serde::Deserialize` with no validation beyond its field types'.

**SV-R-072** — The server area defines no `Error` variant of its own.

---

## UDP

**SV-R-057** — The crate provides `Server::serve_udp`, taking an already-bound UDP socket. Each inbound datagram is dispatched to `Service`'s request-handling method (SV-R-003) independently and its response, if any, is sent to that datagram's source address.

**SV-R-066** — Under `Server::serve_udp` (SV-R-057) no per-peer connection identity is assigned: every datagram reaches the service with the fixed `ConnectionId(0)`, and no connection lifecycle notification (SV-R-030, SV-R-031, SV-R-032, SV-R-033, SV-R-034, SV-R-035, SV-R-036) fires, since a UDP datagram is not part of a connection.

**SV-R-058** — A request-handling failure on one datagram does not affect handling of any other datagram (per-datagram counterpart to SV-R-035's per-connection isolation).

**SV-R-073** — `Service` provides `on_receive_error`, notified when receiving from the socket fails in `serve_udp`, taking the error that failed the receive and answering with an `AcceptErrorAction` (`Continue` or `Stop`). It is not feature-gated.

**SV-R-074** — `on_receive_error` has default behavior of answering `AcceptErrorAction::Continue` when `error.listener_failure()` is `Some(ListenerFailure::Transient)` (TR-R-102) and `AcceptErrorAction::Stop` otherwise.

**SV-R-075** — A datagram that fails to decode under `serve_udp` is never reported to `on_receive_error` (SV-R-058, TR-R-098).

**SV-R-076** — A failure sending one datagram's response under `serve_udp` is never reported to `on_receive_error` (SV-R-058).

**SV-R-077** — On `AcceptErrorAction::Continue` from `on_receive_error`, datagram handling already in flight continues and `serve_udp` receives from the socket again only once `on_receive_error`'s future has completed, so a service backs off by awaiting inside the notification.

**SV-R-078** — On `AcceptErrorAction::Stop` from `on_receive_error`, `serve_udp` returns `Err(error)` with the error that failed the receive, only once every datagram's handling already in flight has finished.

**SV-R-079** — A shutdown requested while `on_receive_error` is pending drops that future without awaiting its completion, and `serve_udp` returns `Ok(())` once every datagram's handling already in flight has finished (SV-R-041, SV-R-042, SV-R-044).

**SV-R-059** — `Service` provides `on_accept_error`, notified when accepting from a listener fails in `serve`, `serve_framed` or `serve_tls`, taking the error that failed the accept and answering with an `AcceptErrorAction` (`Continue` or `Stop`). It has default behavior of answering `Stop`, so an existing implementor is unaffected. No `Connection`/`ConnectionId` is assigned, since no peer was accepted (SV-R-031). A TLS handshake failure is not an accept failure (SV-R-056).

**SV-R-060** — On `AcceptErrorAction::Continue`, serving keeps every live connection and accepts again only once `on_accept_error`'s future has completed, so a service backs off by awaiting inside the notification.

**SV-R-067** — On `AcceptErrorAction::Stop` (SV-R-059), serving drains live connections and returns the error (SV-R-051).

**SV-R-061** — A shutdown requested while `on_accept_error` is pending drops that future without awaiting its completion, and shutdown proceeds as for one requested while accepting (SV-R-041, SV-R-042, SV-R-043, SV-R-044).

**SV-R-062** — `serve_link` returns `Err(error)` when its link ends with `Disconnect::Failed(error)`, and `Ok(())` when it ends with `Disconnect::Closed`, `Disconnect::Rejected` or `Disconnect::ShuttingDown`. The result follows from the same reason passed to `on_disconnect` (SV-R-033). A single link is the whole of what `serve_link` serves, so its failure is serving's failure.

**SV-R-068** — `serve_link` returns only once the `on_disconnect` notification of its link's end (SV-R-062) has completed.
