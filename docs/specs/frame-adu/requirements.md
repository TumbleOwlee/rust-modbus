# Frame ADU — Requirements

Normative behavior of the ADU framings: RTU (with its CRC-16 trailer), RTU over a byte stream, ASCII (with its LRC), TCP (with its MBAP header), and the framing abstraction that unifies them. Split from [`../frame/`](../frame/), which retains PDU structure, the function code taxonomy, exception responses, robustness, buffer reuse, and serde/`Display` support.

This area is **role-agnostic**, on the same terms as `frame/`: it states what is true of a byte sequence regardless of who sent it.

IDs relocated from `frame/` keep their original `FR-R-nnn` numbers unchanged. Requirements added to this area after the split use `FR-ADU-R-nnn`, stable and append-only. See [`../README.md`](../README.md).

Companion documents (shared with `frame/` and [`../frame-data-access/`](../frame-data-access/)): [`../frame/api-contract.md`](../frame/api-contract.md), [`../frame/data-contract.md`](../frame/data-contract.md). This area's own [`edge-cases.md`](./edge-cases.md) holds its boundary and error behavior.

---

## RTU ADU

**FR-R-090** — An RTU ADU consists of a 1-byte address field, the PDU, and a 2-byte CRC, in that order.

**FR-R-091** — An RTU ADU is at most 256 bytes.

**FR-R-092** — The RTU CRC is CRC-16 with the reversed polynomial `0xA001` and an initial value of `0xFFFF`.

**FR-R-093** — The CRC is computed over every byte of the ADU preceding it — the address field and the entire PDU — and over no other bytes.

**FR-R-094** — The CRC is transmitted low byte first, high byte second.

**FR-R-095** — Decoding an RTU ADU whose trailing CRC does not equal the CRC computed over its preceding bytes fails with a checksum error, and the PDU is not decoded.

**FR-R-096** — An RTU address field is an 8-bit value. Address 0 denotes a broadcast to all servers; addresses 1–247 denote individual servers; addresses 248–255 decode without error and are left for the caller to judge.

---

## TCP ADU

**FR-R-100** — A TCP ADU consists of a 7-byte MBAP header followed by the PDU.

**FR-R-101** — The MBAP header consists of a 2-byte transaction identifier, a 2-byte protocol identifier, a 2-byte length field, and a 1-byte unit identifier, each multi-byte field big-endian.

**FR-R-102** — The MBAP protocol identifier is encoded as 0; decoding a header whose protocol identifier is non-zero fails with a protocol-identifier error.

**FR-R-103** — The MBAP length field counts the bytes following it — the unit identifier plus the PDU — and therefore equals `PDU length + 1`.

**FR-R-104** — A TCP ADU is at most 260 bytes.

**FR-R-105** — Decoding a TCP ADU whose MBAP length field is 0, or exceeds 254, fails with an out-of-range error before any allocation proportional to that field is made.

**FR-R-106** — Decoding a TCP ADU fails with a length error if the MBAP length field does not match the number of bytes actually supplied after it.

---

## ASCII ADU

**FR-R-110** — An ASCII ADU consists of a start character `:` (`0x3A`), a 2-character address field, the PDU, a 2-character LRC, and a two-character terminator CR LF (`0x0D 0x0A`), in that order.

**FR-R-111** — Every byte of the address field, the PDU, and the LRC is transmitted as exactly two ASCII hexadecimal characters, most significant nibble first.

**FR-R-112** — Encoding emits hexadecimal characters in uppercase (`0`–`9`, `A`–`F`).

**FR-ADU-R-003** — Decoding an ASCII ADU accepts both uppercase and lowercase hexadecimal characters.

**FR-ADU-R-004** — Decoding an ASCII ADU with any character outside `0`–`9`, `A`–`F`, `a`–`f` in a hexadecimal position fails with an invalid-character error.

**FR-R-113** — An ASCII ADU is at most 513 characters, corresponding to 255 encoded bytes (1 address + 253 PDU + 1 LRC) plus the start character and the two-character terminator.

**FR-R-114** — The LRC is the two's complement of the 8-bit sum of every **decoded byte** preceding it — the address field and the entire PDU — and is computed over the decoded bytes, never over their ASCII hexadecimal characters.

**FR-R-115** — Decoding an ASCII ADU whose LRC does not equal the LRC computed over its preceding decoded bytes fails with a checksum error, and the PDU is not decoded.

**FR-R-116** — Decoding fails with a framing error if the ADU does not begin with `:`, does not end with CR LF, or contains an odd number of hexadecimal characters between them.

**FR-R-117** — The ASCII address field carries the same 8-bit value and the same broadcast and range semantics as the RTU address field (FR-R-096).

**FR-R-118** — The frame layer supports ASCII framing for both the request and the response direction, and for every function code, custom codes and exception responses included. ASCII is a framing choice, not a capability subset.

**FR-R-119** — An ASCII ADU re-encodes to a byte sequence identical to its input up to hexadecimal case: an ADU decoded from lowercase input re-encodes to the uppercase form of the same ADU, which itself round-trips exactly. FR-R-133 applies to the decoded PDU without qualification.

---

## RTU over a byte stream

**FR-R-145** — The frame layer provides a fourth framing, **RTU over stream**, whose ADU is byte-for-byte an RTU ADU (FR-R-090, FR-R-091, FR-R-092, FR-R-093, FR-R-094, FR-R-095, FR-R-096): a 1-byte address field, the PDU, and the CRC of FR-R-092 low byte first. It carries the same header as RTU (FR-R-096), the same maximum ADU length of 256 bytes (FR-R-091), and encodes and decodes identically. It exists to state a different boundary rule, never a different wire format, and no second CRC computation is defined for it.

**FR-R-146** — The extent of an RTU-over-stream ADU is derived from the direction the caller states (FR-R-005), the function code, and the length fields the ADU itself carries, and from nothing else. The derived extent is `3 + PDU length`: the address byte, the PDU, and the two CRC bytes. The derivation is a pure function of the bytes received so far, reports that more bytes are needed rather than reading any, and performs no I/O.

**FR-R-147** — The derivation of FR-R-146 is defined for exactly the function codes and directions in [`../frame/data-contract.md`](../frame/data-contract.md#rtu-over-stream-extents-fr-r-147) `## RTU-over-stream extents (FR-R-147)`, yielding the PDU length that table states.

**FR-R-148** — Any function code or MEI type FR-R-147 does not define fails the derivation with an indeterminate-length error naming the function code, and is never guessed at, scanned for, or terminated by a checksum search. This covers function code 8 in both directions, whose data-word count the specification does not fix (`FR-DA-R-*`, serial-line diagnostics); function code 43 with any MEI type other than 14, whose body is opaque (`FR-DA-R-*`, MEI); and every custom function code, whose body is opaque by definition (FR-R-012). A device using any of them behind a transparent gateway is not reachable through this framing, and the error says so rather than misdelimiting the stream.

**FR-R-149** — An extent the derivation yields that exceeds the framing's maximum ADU length (FR-R-091) fails with the oversized-ADU error before it sizes any read or allocation, on the same terms as FR-R-105.

**FR-R-150** — The RTU-over-stream boundary is not self-locating (FR-R-144). The extent of a frame is read out of that frame's own bytes, so a frame whose content is wrong yields an extent that is wrong, and the position of the next frame is lost with it. A CRC that does not match (FR-R-095) is therefore evidence that the delimitation was itself unsound, and is never treated as a frame-local failure on this framing.

---

## ADU framing abstraction

**FR-R-120** — The frame layer exposes its three framings behind a single abstraction parameterised by the header each carries: RTU and ASCII by a 1-byte address (FR-R-096, FR-R-117), TCP by a transaction identifier and a unit identifier (FR-R-101). Each framing states its maximum ADU length (FR-R-091, FR-R-104, FR-R-113) and provides decode and encode in both directions, the direction stated by the caller as FR-R-005 requires.

**FR-R-121** — Decoding an ADU yields its header and its PDU as separate values. The frame layer never merges the two into a single type, so that a caller may route on the header without re-encoding the PDU.

**FR-R-122** — A framing declares how the end of an ADU is determined, as one of: a length derivable from a fixed-size prefix; a start and end delimiter; inter-frame silence; or a length derivable from the ADU's own content together with the direction it carries. The declaration is a property of the framing and involves no I/O.

---

## Standalone checksum functions

**FR-ADU-R-001** — CRC-16 computation (FR-R-092, FR-R-093, FR-R-094, FR-R-095) is exposed as a public standalone function taking the bytes to check and returning the 16-bit value, so a caller can validate a captured frame's integrity independently of decoding it.

**FR-ADU-R-002** — LRC computation (the ASCII checksum of FR-R-114) is exposed as a public standalone function on the same terms as FR-ADU-R-001.
