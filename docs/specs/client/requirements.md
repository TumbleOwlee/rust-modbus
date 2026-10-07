# Client — Requirements

Normative behavior of the async Modbus client (initiator): the public request API, how requests are issued and responses matched, timeout semantics, retry and reconnect policy, and how protocol exceptions are surfaced to the caller.

Wire encoding is **not** specified here — it belongs to [`../frame/`](../frame/). Socket and serial-port behavior belongs to [`../transport/`](../transport/). This area owns only what is specific to acting as the initiator.

IDs are stable and append-only (`CL-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (public client types, methods, configuration fields), [`edge-cases.md`](./edge-cases.md) (boundary and error behavior, stated limitations).

---

## The client

**CL-R-001** — The client is one type, generic over the framing, serving RTU, ASCII, and TCP alike. Role behavior that differs only by framing is not written three times.

**CL-R-002** — A client is constructed from an established transport, not from an address or a device path. Connecting and opening belong to the transport area (TR-R-020, TR-R-030), and a client does not duplicate them.

**CL-R-003** — The client addresses a server by unit identifier on every request. How that identifier reaches the wire is a property of the framing: the RTU and ASCII header is the identifier itself (FR-R-096, FR-R-117), the TCP header carries it beside a transaction identifier (FR-R-101).

**CL-R-004** — The client is available only when the `std` feature is enabled, since it performs I/O and applies timeouts.

**CL-R-005** — The client permits at most one request in flight at a time, enforced in the type system rather than at run time.

**CL-R-006** — The client surrenders its transport on request, so that a caller may reuse the connection or inspect it after a failure.

---

## Issuing a request

**CL-R-010** — Issuing a request builds the framing's header from the unit identifier and the transaction identifier, encodes the ADU, writes it in full, and then awaits a response.

**CL-R-011** — The client allocates transaction identifiers itself. The first is 1, each subsequent one is the previous plus one, and the sequence wraps from 65535 to 1. Zero is never allocated, so that a matched response is never matched against an unset field.

**CL-R-012** — A request that cannot be encoded fails without writing any bytes to the transport.

**CL-R-013** — A partially written request is never retried or abandoned mid-ADU: the write is driven to completion or the client is unusable thereafter (CL-R-031). A truncated ADU on the wire desynchronizes the peer, which no later request can repair.

**CL-R-014** — The response deadline starts when the request has been written, not when it was submitted. Time spent writing does not consume the time allowed for a reply.

---

## Matching a response

**CL-R-020** — A response is accepted only if its header corresponds to the header sent. For RTU and ASCII the unit identifier is equal; for TCP both the transaction identifier and the unit identifier are equal.

**CL-R-021** — A response whose header does not correspond is discarded and the client continues waiting.

**CL-R-097** — Discarding a non-corresponding response (CL-R-021) does not extend the deadline of CL-R-014.

**CL-R-022** — A response whose header corresponds but whose function code is neither the code requested nor an exception to it fails immediately with an unexpected-function error, naming the code expected and the code received.

**CL-R-023** — A response that cannot be decoded fails with the frame area's decoding error unaltered.

**CL-R-098** — A decoding failure (CL-R-023) leaves the client desynchronized only where the framing is not self-locating (FR-R-144): where a length field alone delimits frames, a malformed ADU leaves the reader unable to find the next one. On a self-locating framing the failure costs exactly that frame — the client remains usable and a subsequent request proceeds normally.

**CL-R-099** — A request whose response failed to decode (CL-R-023) does not wait further for another response: the frame that failed was the answer to it.

**CL-R-024** — A response arriving after its request has timed out is never delivered as the result of a later request. It is either discarded by CL-R-021 or refused by CL-R-031.

---

## Timeouts and desynchronization

**CL-R-030** — The client bounds the wait for a response by a configurable response timeout.

**CL-R-100** — The default response timeout of CL-R-030 is 1 second.

**CL-R-031** — A response timeout or an I/O failure marks the client desynchronized: what the peer will send next is no longer known. A decoding failure marks it desynchronized only in the case CL-R-098 names.

**CL-R-032** — A desynchronized client fails every subsequent request immediately, without writing to the transport, with a distinct error naming the condition.

**CL-R-033** — Recovery from desynchronization is by discarding the client and establishing a new transport. The client never silently resynchronizes by draining, reconnecting, or retrying — each would issue a request the caller did not authorize.

**CL-R-034** — The client reports whether it is desynchronized, so that a caller may discard it without first provoking an error.

**CL-R-101** — The desynchronization report of CL-R-034 is a projection of the client state of CL-R-035 rather than a value maintained beside it, so the two never disagree.

**CL-R-035** — The client reports a state value describing what it knows about its own usability, distinguishing four cases: that no exchange has yet been attempted; that the last exchange was answered by the peer; that the last exchange was not answered and the client remains usable; and that the client is unusable and will refuse every further request (CL-R-032).

**CL-R-036** — An exchange counts as answered when a frame corresponding to the request was received and decoded, including an exception response (CL-R-040) and a response carrying another function's code (CL-R-022): in each case the peer answered, whatever it said.

**CL-R-102** — A broadcast write leaves the reported state (CL-R-035) unchanged rather than count as answered (CL-R-036), since no server replies to a broadcast (CL-R-051) and nothing was therefore heard from any peer.

**CL-R-037** — Where the client is unusable, the state names what the client observed at the moment it became so, distinguishing: that the peer's end of stream was seen; that the platform reported another I/O failure; that the response timeout elapsed with no matching response; and that a frame failed to decode on a framing that is not self-locating (CL-R-098). The reason names the observation and asserts nothing about the peer's condition, which the client cannot observe.

**CL-R-038** — The reported state is derived only from observations the client has already made. Reporting it does not read from or write to the transport, does not block, and does not change the state it reports.

**CL-R-039** — The client offers no liveness probe. Establishing that a peer still answers requires issuing a request, and which requests reach the wire is the caller's to authorize (CL-R-033); a caller that wants a probe issues it through the raw request method (CL-R-061).

---

## Exceptions

**CL-R-040** — A response that is an exception to the function requested is surfaced to a typed request method as a failure carrying both the function code and the exception code, never as a success value.

**CL-R-041** — An exception code outside those named by the frame area is surfaced unaltered rather than rejected: it is a legal response the server chose to send (FR-R-072).

**CL-R-042** — Receiving an exception leaves the client usable. The exchange completed; the server merely refused the request.

---

## Broadcast

**CL-R-050** — Whether a unit identifier is a broadcast address is a property of the framing: unit 0 broadcasts on RTU and ASCII (FR-R-096), and no identifier broadcasts on TCP.

**CL-R-051** — A write request addressed to a broadcast identifier is written and completes without awaiting a response, since no server replies to a broadcast.

**CL-R-052** — A read request addressed to a broadcast identifier fails before anything is written, since a read whose answer cannot arrive is a caller error, not a silent no-op.

**CL-R-053** — Issuing a raw request to a broadcast identifier succeeds with no response value rather than fail, so the raw path can express a broadcast that the typed methods forbid.

---

## The request API

**CL-R-060** — The client exposes one typed method per named function code of FR-R-010, taking and returning the domain value types of FR-R-007 rather than bare integers.

**CL-R-061** — The client exposes a raw method taking a request PDU and yielding the response PDU as received, including an exception response. This is how a custom function code (FR-R-011) is issued and how a caller inspects a response the typed methods interpret.

**CL-R-062** — A typed method reading bits returns exactly the quantity of bits requested, discarding the padding bits of the final byte (FR-R-024).

**CL-R-063** — The client does not validate request field ranges itself. Range rules are frame-area behavior (FR-R-021, FR-R-027, FR-R-031) and surface from encoding, so one rule has one home.

**CL-R-064** — A typed method does not compare the fields a server echoes against the fields sent. An echo mismatch is a server defect the caller may detect via CL-R-061; the client never fails a request over it.

**CL-R-065** — Behind the crate's `serde` feature, `ClientConfig` implements `serde::Serialize` and `serde::Deserialize` with no validation beyond its field types'.

**CL-R-103** — The serde representation of `ClientConfig` (CL-R-065) keeps `Duration`'s own serde representation for `response_timeout` rather than a count in any single unit, so that every value `Duration` can hold survives a round trip exactly — including one whose nanosecond count would not fit an integer field.

---

## The blocking client

**CL-R-070** — The crate provides a blocking client, gated behind an off-by-default `sync` feature that implies `std`.

**CL-R-071** — The blocking client exposes one method for every request method of the async client: each typed method of CL-R-060 and the raw method of CL-R-061. No request issuable through the async client is unreachable from the blocking one, and no request method exists on one surface and not the other.

**CL-R-072** — Every guarantee the async client makes holds identically in the blocking one: the response timeout of CL-R-030, the desynchronization rules of CL-R-031, CL-R-032 and CL-R-033, the broadcast rules of CL-R-050, CL-R-051, CL-R-052 and CL-R-053, and the exception surfacing of CL-R-040, CL-R-041 and CL-R-042. The blocking client obtains them by delegating to the async client, not by reimplementing any of them.

**CL-R-073** — The blocking client owns the runtime it drives, built with every driver the code beneath it uses, so that no configured timeout can fail for want of one.

**CL-R-104** — Constructing a blocking client does not require the caller to possess a runtime (CL-R-073).

**CL-R-074** — No type belonging to the async runtime appears in an argument or return position of any blocking method.

**CL-R-105** — Every blocking client type is nameable through this crate alone (as TR-R-034 requires of the serial stream).

**CL-R-075** — A blocking method called from a thread that is already driving a runtime returns a distinct error before touching the transport, rather than panicking or deadlocking.

**CL-R-076** — The blocking client provides its own constructors, taking a socket address or a device path together with the transport and client configuration, since a caller with no runtime cannot construct a transport to hand in (CL-R-002 binds the async client only).

**CL-R-077** — Sequential blocking calls are correct with no delay between them. When a blocking call returns, the exchange is fully settled and the next call proceeds without a sleep, a drain, or a reset by the caller.

**CL-R-078** — The blocking client reports the state of CL-R-034 and CL-R-035 without entering its runtime, since CL-R-038 forbids reporting that blocks.

**CL-R-079** — No blocking server is provided. A server is driven by inbound connections rather than by caller-issued calls, so the thread structure that would serve it is the caller's choice and not this crate's.

---

## Transport genericity

**CL-R-080** — `Client<T, F>` is generic over any `T: ClientTransport<F>` (TR-R-075), not only `FrameTransport<S, F>`. `TcpClient`, `RtuOverTcpClient`, `RtuClient`, and `AsciiClient` keep their existing public names and behavior under this change.

**CL-R-081** — The crate provides `UdpClient = Client<UdpTransport<Tcp>, Tcp>`, mirroring `TcpClient`'s naming, so a caller can build a `Client`-compatible UDP client directly from `connect_udp`'s output.

---

## Pipelined clients

**CL-R-082** — The crate provides `PipelinedClient`, a cloneable handle over a TCP transport permitting several requests in flight concurrently, distinguished by MBAP transaction id, coexisting with `Client<T, F>` rather than replacing it.

**CL-R-106** — RTU and ASCII remain served only by `Client<T, F>`, never by a pipelined handle (CL-R-082), since their framings carry no transaction id and cannot distinguish concurrent responses.

**CL-R-083** — The crate provides `PipelinedUdpClient`, the same concurrent-request handle as CL-R-082 built over `UdpTransport<Tcp>`, mirroring `PipelinedClient`'s API.

**CL-R-084** — `PipelinedClient` and `PipelinedUdpClient` are backed by a background task, spawned on construction, that owns the transport and dispatches each response to the caller awaiting its transaction id.

**CL-R-085** — A `PipelinedClient`/`PipelinedUdpClient` handle is `Clone`; every clone shares the same background task and transport.

**CL-R-107** — When the last `PipelinedClient`/`PipelinedUdpClient` handle clone (CL-R-085) is dropped, the background task shuts down and the transport closes.

**CL-R-086** — Both types expose a `send` method that, when the number of in-flight requests has reached the configured `max_in_flight`, awaits a free slot before writing to the transport.

**CL-R-108** — Both types expose a `try_send` method that, when the number of in-flight requests has reached the configured `max_in_flight`, fails immediately with a distinct error naming the condition instead of awaiting a slot (CL-R-086).

**CL-R-087** — Dropping the future returned by `send`/`try_send` before it resolves frees its slot in the in-flight table; no explicit cancellation method is required.

**CL-R-088** — A response whose transaction id belonged to a request already resolved by timeout or desynchronization is discarded and does not affect any other in-flight request.

**CL-R-089** — A response whose transaction id was never issued by this handle desynchronizes the connection, on both `PipelinedClient` and `PipelinedUdpClient` — unlike CL-R-090/091's transport-specific timeout handling, this case indicates a genuine correctness violation (a garbled echo, crossed wires, or an injected frame) rather than routine packet loss, so no UDP exception applies. Distinct from CL-R-088's silent discard.

**CL-R-090** — On `PipelinedClient`, a response timeout on any one in-flight request desynchronizes the whole connection, immediately failing every other in-flight request with the same desynchronized error, and refusing every subsequent request without writing to the transport — the same posture as CL-R-031/032, extended from one request to every request sharing the connection.

**CL-R-091** — On `PipelinedUdpClient`, a response timeout on one in-flight request fails only that request; it does not desynchronize the connection and does not affect any other in-flight request.

**CL-R-109** — On `PipelinedUdpClient`, an I/O failure desynchronizes the whole connection, as CL-R-031 states, unlike the timeout of CL-R-091.

**CL-R-092** — Recovery from a desynchronized `PipelinedClient`/`PipelinedUdpClient` is by discarding the handle and constructing a new one, exactly as CL-R-033 states for `Client<T, F>`; neither type silently resynchronizes.

**CL-R-093** — The crate provides `PipelineConfig`, distinct from `ClientConfig`, carrying `response_timeout` (defaulting to 1 second, mirroring CL-R-100) and `max_in_flight` (defaulting to 16, with a hard ceiling of 65535 — the MBAP transaction id space) bounding the in-flight table.

**CL-R-094** — `PipelinedClient` and `PipelinedUdpClient` are gated behind an off-by-default `pipeline` feature that implies `std`.

**CL-R-095** — `ClientConfig` implements `From<ClientConfig> for PipelineConfig`, carrying over `response_timeout` and setting `max_in_flight` to CL-R-093's default, so a caller moving from `Client` to a pipelined type is not required to reconstruct configuration it already had.

**CL-R-096** — `PipelinedClient` and `PipelinedUdpClient` expose `is_desynchronized(&self) -> bool`, reporting whether the handle currently refuses every request, mirroring CL-R-034.
