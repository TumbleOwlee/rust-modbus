# Transport — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**.
Entries under "Known limitations" are working as implemented; they are recorded
here so they are not mistaken for oversights and silently "fixed".

---

## 1. Framing

| Condition | Behavior |
|---|---|
| Two ADUs arrive in one read | Both are delivered, one per `recv` call; the surplus is retained, never discarded (TR-R-004) |
| A read splits a TCP ADU anywhere | Reading resumes until the ADU is complete; a split MBAP header is not an error (TR-R-010) |
| An RTU frame contains gaps shorter than 3.5 character times | Treated as one frame. The t1.5 intra-character rule is **not** enforced — see limitations |
| An RTU idle gap on an in-memory pair | Detected exactly as on a real port: the rule is implemented as a read timeout, not as a UART property (TR-R-011) |
| Bytes before an ASCII `:` | Discarded silently, up to the maximum ADU length, then an oversized-ADU error (TR-R-012, TR-R-013) |
| An ASCII frame with no terminator, followed by silence | Read until the ADU maximum, then an oversized-ADU error; silence alone does not terminate an ASCII frame |
| A frame fails to decode | Exactly that frame's bytes are consumed, the error surfaces, and the transport stays usable (TR-R-005) |
| An ADU claims or occupies more than `MAX_ADU_LEN` | Oversized-ADU error; the read buffer never grows past that bound (TR-R-013) |
| A receive fails before the ADU was delimited, RTU or ASCII | The bytes gathered for the attempt are discarded, so the next receive starts at the next boundary the wire provides (TR-R-044) |
| A receive fails before the ADU was delimited, TCP | The gathered bytes are retained; without the length field there is no later boundary to resume from, so the failure is terminal for the stream (TR-R-044) |
| An RTU-over-TCP ADU split across any number of reads | Reassembled: the derivation reports that more bytes are needed until the extent is known and in hand (TR-R-045) |
| Two RTU-over-TCP ADUs coalesced into one read | Both delivered, one per `recv` call; the extent of the first is what separates them, not a gap (TR-R-045, TR-R-004) |
| An RTU-over-TCP frame whose extent cannot be derived | Indeterminate-length error; the gathered bytes are retained and the failure is terminal for the stream (TR-R-046, FR-R-148) |
| An idle gap in an RTU-over-TCP stream | Ignored entirely; the inter-frame interval has no effect over a socket (TR-R-048) |
| A transport that only ever receives | Never allocates a write buffer at all; the cost is paid on the first send (TR-R-043) |
| An idle transport between sends | Keeps its write buffer's capacity — up to `MAX_ADU_LEN` — resident on purpose; that retention *is* the reuse (TR-R-043) |
| A send that fails mid-write | The write buffer is cleared before the next frame, so no fragment of the abandoned ADU is ever re-sent (TR-R-043, FR-R-142) |

## 2. Connection lifecycle

| Condition | Behavior |
|---|---|
| Connection refused | The I/O error, carrying `ErrorKind::ConnectionRefused` (TR-R-040) — deliberately distinct from a connect timeout (TR-R-021) |
| Connect timeout expires | The timeout error naming `"connect"`, with no I/O error underneath it, since none occurred |
| Peer closes between two ADUs | `Io { kind: UnexpectedEof }` — the stream ended and no frame was lost (TR-R-014) |
| Peer closes part-way through an ADU | `ConnectionClosed`; the partial bytes are dropped (TR-R-014) |
| A serial peer closes immediately after a complete RTU frame | The frame is delivered; a close after a whole ADU is not a severed one |
| Peer resets the connection | The I/O error carrying `ErrorKind::ConnectionReset` |
| A receive times out mid-ADU | The transport is desynchronized: the caller must reconnect rather than receive again (TR-R-041) |
| Serial device disappears mid-session | Whatever `ErrorKind` the platform reports, surfaced through the I/O error; the transport is not usable afterwards |

Both cases above are errors, because the receive methods return `Result` with no
vacant success value; they are distinguished by variant, which is what TR-R-014
requires.

Sending imposes no timeout of its own, and receiving imposes none beyond the RTU
inter-frame interval: per-request timing belongs to the client (TR-R-042). A
caller wanting a bounded receive wraps it in `tokio::time::timeout` — and then
owns the desynchronization that TR-R-041 describes.

## 3. Known limitations

- **The RTU t1.5 intra-character timeout is not enforced.** The Modbus serial
  specification calls for a frame to be rejected when more than 1.5 character
  times elapse *within* it. Enforcing that from a buffered async byte stream is
  not reliably possible — the OS and the driver coalesce reads, so intra-frame
  gaps are invisible by the time bytes arrive. Only the t3.5 inter-frame silence
  of TR-R-011 is enforced, which is what determines a boundary; a frame corrupted
  by an intra-character gap is instead caught by its CRC (FR-R-095).
- **"No allocation while sending" means the transport's own encoding.** TR-R-043
  bounds what this crate allocates: one reused buffer, filled in place. A stream
  underneath may allocate on its own account — `tokio::io::duplex` allocates per
  write by design — and that is the stream's business, not the transport's. The
  test that pins TR-R-043 therefore writes into a stream that allocates nothing,
  so the count it reports is ours.
- **No UDP.** Modbus over UDP is not part of the specification, and a datagram
  transport has different framing and retransmission semantics than the stream
  the client and server are written against.
- **RTU-over-TCP costs the whole link, not one frame.** The mode is supported
  (FR-R-145), but its boundary is derived from each frame's own content, so it is
  not self-locating (FR-R-150) and one corrupted or undecodable frame
  desynchronizes the client (CL-R-023) or ends the server's connection
  (SV-R-050) — the same posture as Modbus TCP, and unlike RTU on a serial line,
  where the silence would still be there to resynchronize on. A deployment that
  expects noise on the far side of the gateway should expect to reconnect.
- **Serial parity and framing errors are not reported per byte.** The serial
  backend surfaces them, at best, as an I/O error covering an entire read. A byte
  corrupted in a way the UART detected is therefore indistinguishable here from
  one corrupted silently; both are caught by the CRC or LRC.
- **ASCII framing has no inter-character timeout.** The specification gives ASCII
  mode a configurable inter-character timeout, defaulting to one second, after
  which a partial frame is abandoned. TR-R-012 terminates an ASCII frame on CR LF
  and on the ADU maximum only, so a stalled sender holds the receive pending
  until the caller's own timeout fires rather than being abandoned at one second.
