# Frame ADU — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**, for the RTU, RTU-over-stream, ASCII, and TCP framings. Core PDU/exception/buffer edge cases live in [`../frame/edge-cases.md`](../frame/edge-cases.md); data-access edge cases live in [`../frame-data-access/edge-cases.md`](../frame-data-access/edge-cases.md).

Everything in `## Known limitations` is working as specified; it is recorded here so it is not mistaken for an oversight and silently "fixed".

---

## ADU decode boundaries

| ID | Condition | Behavior |
|---|---|---|
| **FR-ADU-E-001** | RTU CRC mismatch | checksum error; PDU not decoded (FR-R-095) |
| **FR-ADU-E-002** | RTU address 248–255 | decodes; the caller judges (FR-R-096) |
| **FR-ADU-E-003** | RTU address 0 | decodes as broadcast; sending no response is server-area behavior (FR-R-096) |
| **FR-ADU-E-004** | ASCII ADU missing `:` or CR LF | framing error (FR-R-116) |
| **FR-ADU-E-005** | ASCII odd hexadecimal character count | framing error (FR-R-116) |
| **FR-ADU-E-006** | Non-hexadecimal character in a hexadecimal position | invalid-character error (FR-ADU-R-004) |
| **FR-ADU-E-007** | ASCII LRC mismatch | checksum error; PDU not decoded (FR-R-115) |
| **FR-ADU-E-008** | Lowercase ASCII hexadecimal input | accepted; re-encodes uppercase (FR-ADU-R-003, FR-R-112, FR-R-119) |
| **FR-ADU-E-009** | MBAP protocol identifier ≠ 0 | protocol-identifier error (FR-R-102) |
| **FR-ADU-E-010** | MBAP length field 0, or > 254 | length error, raised before any sizing allocation (FR-R-105) |
| **FR-ADU-E-011** | MBAP length field disagreeing with the bytes supplied | length error (FR-R-106) |
| **FR-ADU-E-012** | Any ADU exceeding its framing's maximum | size error (FR-R-091, FR-R-104, FR-R-113) |
| **FR-ADU-E-013** | RTU-over-stream ADU, function code 8, 43 with MEI ≠ 14, or a custom code | indeterminate-length error naming the function code; the extent is not guessed (FR-R-148) |
| **FR-ADU-E-014** | RTU-over-stream exception response to any function code, custom included | extent is 5 bytes; the exception path stays derivable where the normal path is not (FR-R-147, FR-R-086) |
| **FR-ADU-E-015** | RTU-over-stream derived extent above 256 bytes | oversized-ADU error, raised before it sizes any read (FR-R-149) |
| **FR-ADU-E-016** | RTU-over-stream CRC mismatch | checksum error, and the delimitation is unsound with it: the failure is terminal for the stream, not frame-local (FR-R-150) |

Every row above is an error, never a panic: FR-R-130 admits no exception for any input whatsoever.

## Known limitations

- **FR-ADU-E-017** — **ASCII terminators are strict.** Only CR LF is accepted. The Modbus serial specification permits a configurable end-of-frame character in some implementations; that configurability is not offered, so a peer using a non-standard terminator does not interoperate.
- **FR-ADU-E-018** — **ASCII framing is specified at the frame layer; operating a serial port in ASCII mode, including its inter-character timeout, is transport-area behavior (TR-R-076).** This area only encodes and decodes the ADU.
- **FR-ADU-E-019** — **Broadcast is recognised, not enforced.** FR-R-096 names address 0; the rule that a server sends no response to a broadcast is server-area behavior.
- **FR-ADU-E-020** — **RTU over a stream cannot carry every function code.** The boundary is derived from the frame's own length fields, and function code 8, function code 43 outside MEI type 14, and every custom code have no derivable length (FR-R-148). They encode and decode perfectly well; what cannot be done is find where they end in a byte stream that gives no other clue. This is a property of the mode, not of this implementation: a transparent gateway forwards bytes and adds nothing to delimit them, so any stack reading them either derives the length as this one does, guesses, or scans for a CRC that matches by luck.
- **FR-ADU-E-021** — **No Modbus Plus, and no serial-line ASCII delimiter negotiation.** Four framings only: RTU, RTU over stream, ASCII, TCP.
