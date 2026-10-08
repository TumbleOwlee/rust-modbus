# Transport — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**. Entries under "Known limitations" are working as implemented; they are recorded here so they are not mistaken for oversights and silently "fixed".

---

## Framing

| ID | Condition | Behavior |
|---|---|---|
| **TR-E-001** | Two ADUs arrive in one read | Both are delivered, one per `recv` call; the surplus is retained, never discarded (TR-R-004, TR-R-082) |
| **TR-E-002** | A read splits a TCP ADU anywhere | Reading resumes until the ADU is complete; a split MBAP header is not an error (TR-R-010) |
| **TR-E-003** | An RTU frame contains gaps shorter than 3.5 character times | Treated as one frame. The t1.5 intra-character rule is **not** enforced — see TR-E-035 |
| **TR-E-004** | An RTU idle gap on an in-memory pair | Detected exactly as on a real port: the rule is implemented as a read timeout, not as a UART property (TR-R-011) |
| **TR-E-005** | Bytes before an ASCII `:` | Discarded silently and without limit on their count; no error is raised, and memory held stays within the maximum ADU length (TR-R-087, TR-R-013) |
| **TR-E-006** | An ASCII frame with no terminator, followed by silence past the inter-character timeout | Abandoned: the timeout error (TR-R-076), the gathered bytes discarded (TR-R-044) |
| **TR-E-007** | A frame fails to decode | Exactly that frame's bytes are consumed, the error surfaces, and the transport stays usable (TR-R-005) |
| **TR-E-008** | An ADU claims or occupies more than `MAX_ADU_LEN` | Oversized-ADU error; the read buffer never grows past that bound (TR-R-013) |
| **TR-E-009** | A receive fails before the ADU was delimited, RTU or ASCII | The bytes gathered for the attempt are discarded, so the next receive starts at the next boundary the wire provides (TR-R-044) |
| **TR-E-010** | A receive fails before the ADU was delimited, TCP | The gathered bytes are retained; without the length field there is no later boundary to resume from, so the failure is terminal for the stream (TR-R-083) |
| **TR-E-011** | An RTU-over-TCP ADU split across any number of reads | Reassembled: the derivation reports that more bytes are needed until the extent is known and in hand (TR-R-045) |
| **TR-E-012** | Two RTU-over-TCP ADUs coalesced into one read | Both delivered, one per `recv` call; the extent of the first is what separates them, not a gap (TR-R-045, TR-R-004, TR-R-082) |
| **TR-E-013** | An RTU-over-TCP frame whose extent cannot be derived | Indeterminate-length error; the gathered bytes are retained and the failure is terminal for the stream (TR-R-046, FR-R-148) |
| **TR-E-014** | An idle gap in an RTU-over-TCP stream | Ignored entirely; the inter-frame interval has no effect over a socket (TR-R-048) |
| **TR-E-015** | A transport that only ever receives | Never allocates a write buffer at all; the cost is paid on the first send (TR-R-043) |
| **TR-E-016** | An idle transport between sends | Keeps its write buffer's capacity resident on purpose; that retention *is* the reuse (TR-R-043). Normally `MAX_ADU_LEN`; a failed oversized encode may leave it larger, while the buffer's length stays within `MAX_ADU_LEN` (TR-R-084) |
| **TR-E-017** | A send that fails mid-write | The write buffer is cleared before the next frame, so no fragment of the abandoned ADU is ever re-sent (TR-R-043, FR-R-142) |

## Connection lifecycle

| ID | Condition | Behavior |
|---|---|---|
| **TR-E-018** | Connection refused | The I/O error, carrying `ErrorKind::ConnectionRefused` (TR-R-040) — deliberately distinct from a connect timeout (TR-R-089) |
| **TR-E-019** | Connect timeout expires | The timeout error naming `"connect"`, with no I/O error underneath it, since none occurred (TR-R-089) |
| **TR-E-020** | Peer closes between two ADUs | `Io { kind: UnexpectedEof }` — the stream ended and no frame was lost (TR-R-014) |
| **TR-E-021** | Peer closes part-way through an ADU | `ConnectionClosed`; the partial bytes are dropped (TR-R-088) |
| **TR-E-022** | A serial peer closes immediately after a complete RTU frame | The frame is delivered; a close after a whole ADU is not a severed one |
| **TR-E-023** | Peer resets the connection | The I/O error carrying `ErrorKind::ConnectionReset` |
| **TR-E-024** | A receive times out mid-ADU | The transport is desynchronized: a further receive fails, and recovery is a reconnect (TR-R-090) |
| **TR-E-025** | Serial device disappears mid-session | Whatever `ErrorKind` the platform reports, surfaced through the I/O error; the transport is not usable afterwards |

- **TR-E-026** — Both peer-close cases (TR-E-020, TR-E-021) are errors, because the receive methods return `Result` with no vacant success value; they are distinguished by variant, which is what TR-R-014 and TR-R-088 require.
- **TR-E-027** — Sending imposes no timeout of its own; receiving imposes none beyond RTU's inter-frame interval and ASCII's inter-character timeout (TR-R-076): per-request timing belongs to the client (TR-R-042). A caller wanting a bounded receive wraps it in `tokio::time::timeout` — and then owns the desynchronization that TR-R-090 describes.

## RS-485 kernel direction control

| ID | Condition | Behavior |
|---|---|---|
| **TR-E-028** | `rs485` requested on a non-Linux target | `Error::Rs485Unsupported` at `open_serial` time; no ioctl is attempted (TR-R-054) |
| **TR-E-029** | The driver accepts `TIOCSRS485` but the hardware ignores a field | Not detectable. The ioctl reports success on write; the crate cannot read the delays back and would not know a mismatch if it could |
| **TR-E-030** | A `Duration` finer than one millisecond passed as a delay | Rounded down to whole milliseconds at the ioctl boundary (TR-R-056); the kernel field has no finer resolution |
| **TR-E-031** | Whether the line physically turned around | Never verified by this crate. `TIOCSRS485` configures the driver's intent; a scope on the bus is the only real verification |
| **TR-E-032** | An independently-set after-send RTS polarity | Not offered (TR-R-057). The kernel struct allows it; no two-wire board this crate targets wants it |
| **TR-E-033** | `rs485: None` on a port where RS-485 mode is already enabled — by the device tree, by `setserial`, or by an earlier process | The mode stays enabled. `None` issues no ioctl at all (TR-R-092) rather than issuing one that clears `SER_RS485_ENABLED`, so this crate can turn RS-485 on but never off. Disabling it is the operator's job, outside this crate |

## Serde

| ID | Condition | Behavior |
|---|---|---|
| **TR-E-034** | A deserialized `SerialConfig` with `baud_rate: 0` | Accepted at deserialize time; rejected the first time it is asked for an inter-frame interval or to open a port, exactly as a hand-built zero-baud config is today (TR-R-095). Validating earlier would give the serde path stricter semantics than the constructor, which this crate has never had |

## Known limitations

- **TR-E-035** — **The RTU t1.5 intra-character timeout is not enforced.** The Modbus serial specification calls for a frame to be rejected when more than 1.5 character times elapse *within* it. Enforcing that from a buffered async byte stream is not reliably possible — the OS and the driver coalesce reads, so intra-frame gaps are invisible by the time bytes arrive. Only the t3.5 inter-frame silence of TR-R-011 is enforced, which is what determines a boundary; a frame corrupted by an intra-character gap is instead caught by its CRC (FR-R-095).
- **TR-E-036** — **"No allocation while sending" means the transport's own encoding.** TR-R-043 bounds what this crate allocates: one reused buffer, filled in place. A stream underneath may allocate on its own account — `tokio::io::duplex` allocates per write by design — and that is the stream's business, not the transport's. The test that pins TR-R-043 therefore writes into a stream that allocates nothing, so the count it reports is ours.
- **TR-E-037** — **UDP has no transport-level reliability.** `UdpTransport` and `Server::serve_udp` (TR-R-070, SV-R-057) do no retransmission, acknowledgement, or sequencing of datagrams — a lost or reordered datagram is invisible at this layer and relies entirely on the client's own retry (`CL-R-*`) to recover. An ADU too large for one UDP datagram is refused at encode time (TR-R-097); the transport does not fragment or reassemble across multiple datagrams.
- **TR-E-038** — **RTU-over-TCP costs the whole link, not one frame.** The mode is supported (FR-R-145), but its boundary is derived from each frame's own content, so it is not self-locating (FR-R-150) and one corrupted or undecodable frame desynchronizes the client (CL-R-098) or ends the server's connection (SV-R-050) — the same posture as Modbus TCP, and unlike RTU on a serial line, where the silence would still be there to resynchronize on. A deployment expecting noise on the far side of the gateway plans for reconnects.
- **TR-E-039** — **Serial parity and framing errors are not reported per byte.** The serial backend surfaces them, at best, as an I/O error covering an entire read. A byte corrupted in a way the UART detected is therefore indistinguishable here from one corrupted silently; both are caught by the CRC or LRC.
- **TR-E-040** — **`AduReader`'s `Direction` is the caller's to match to the physical stream.** Over RTU-over-stream, boundary derivation depends on `Direction` (TR-R-078); constructing a reader with the wrong one for the stream it reads — one reader per TCP connection half — risks the same whole-link desync TR-R-046/FR-R-150 describe for a live transport. This is the existing risk, reached through a new door, not a new one.
- **TR-E-041** — **`ClientTransport` futures are `Send` (TR-R-081).** An implementor whose future is not `Send` no longer compiles; the in-crate impls require `F: Send` and `F::Header: Sync`, and a caller spawning `Client::call` generically needs `F::Header: Send` as well. Every in-crate transport is tokio I/O or plain data, so these hold trivially — the cost lands only on exotic external implementors. Tightening a public trait bound is breaking for external implementors: minor-version bump under 0.x.
- **TR-E-048** — **No classification table outside Unix and Windows.** On any other platform every `Io` classifies as `Fatal` (TR-R-103), since the crate carries no errno table for it; a service there that wants to retry decides on `kind`/`raw_os_error` itself.
- **TR-E-049** — **An `Io` without an OS code is `Fatal`, whatever its kind.** Classification reads `raw_os_error` only (TR-R-102, TR-R-103); an `Io { kind: WouldBlock, raw_os_error: None }` built from a synthetic `std::io::Error` is `Fatal`. The OS code is the only input that separates `ENOBUFS` from `EBADF`, both of which std reports as the same kind.

## TLS

- **TR-E-042** — **A server cert rejected by `Verify`** (untrusted issuer, expired, wrong name) fails the handshake as `Error::TlsHandshake`; no connection is ever established (TR-R-065, TR-R-067).
- **TR-E-043** — **A client cert rejected under `ClientCertPolicy::Require`** fails on the server side as `Error::TlsHandshake { source, peer_cert }`, `peer_cert` `Some` with the offered certificate (TR-R-069). No `Connection` identity was ever assigned, so the attempt reaches neither `Service::on_connect` nor `on_error` — it reaches `Service::on_tls_handshake_failed(peer, &error)` instead, the notification dedicated to a handshake that fails before any `Connection` exists (SV-R-055, SV-R-056, TR-R-066).
- **TR-E-044** — **A client cert accepted under `ClientCertPolicy::AllowAny`** is never chain-validated; `accept`/`accept_framed`'s returned `peer_cert` is `Some` with whatever certificate was presented, without any trust decision behind it. A handshake with no cert presented at all still fails exactly as under `Require` (TR-R-069's `peer_cert: None` case), since the cert is still mandatory — only chain validation is skipped.
- **TR-E-045** — **The connect timeout bounds the whole `connect_tls` call.** `TcpConfig`'s `connect_timeout` (TR-R-021) covers the TCP connect and the TLS handshake together — there is no second, TLS-specific timeout knob. Expiry is always `Error::Timeout{what:"connect"}`, never `Error::TlsHandshake`, even when the TCP connect itself succeeded and only the handshake stalled.
- **TR-E-046** — **`no_std`/`tls` interaction is structurally unreachable, not separately tested.** `tls` implies `std` (TR-R-060), same as `rtu`/`rs485`; the `no-std` CI job builds with no features at all, so there is nothing to exercise here.
- **TR-E-047** — **`RootStore::native()`/`Default` fail closed.** Loading the platform's trust store is best-effort: an individual unreadable native cert is skipped, and a platform with no discoverable store at all yields an empty `RootStore`, which fails every `Verify` rather than trusting nothing.
