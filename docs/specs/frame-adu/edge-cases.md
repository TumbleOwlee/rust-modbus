# Frame ADU — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**,
for the RTU, RTU-over-stream, ASCII, and TCP framings. Core PDU/exception/buffer
edge cases live in [`../frame/edge-cases.md`](../frame/edge-cases.md);
data-access edge cases live in
[`../frame-data-access/edge-cases.md`](../frame-data-access/edge-cases.md).

Everything in §2 is working as specified; it is recorded here so it is not
mistaken for an oversight and silently "fixed".

---

## 1. ADU decode boundaries

| Condition | Behavior |
|---|---|
| RTU CRC mismatch | checksum error; PDU not decoded (FR-R-095) |
| RTU address 248–255 | decodes; the caller judges (FR-R-096) |
| RTU address 0 | decodes as broadcast; sending no response is server-area behavior |
| ASCII ADU missing `:` or CR LF | framing error (FR-R-116) |
| ASCII odd hexadecimal character count | framing error (FR-R-116) |
| Non-hexadecimal character in a hexadecimal position | invalid-character error (FR-R-112) |
| ASCII LRC mismatch | checksum error; PDU not decoded (FR-R-115) |
| Lowercase ASCII hexadecimal input | accepted; re-encodes uppercase (FR-R-112, FR-R-119) |
| MBAP protocol identifier ≠ 0 | protocol-identifier error (FR-R-102) |
| MBAP length field 0, or > 254 | length error, raised before any sizing allocation (FR-R-105) |
| MBAP length field disagreeing with the bytes supplied | length error (FR-R-106) |
| Any ADU exceeding its framing's maximum | size error (FR-R-091, FR-R-104, FR-R-113) |
| RTU-over-stream ADU, function code 8, 43 with MEI ≠ 14, or a custom code | indeterminate-length error naming the function code; the extent is not guessed (FR-R-148) |
| RTU-over-stream exception response to any function code, custom included | extent is 5 bytes; the exception path stays derivable where the normal path is not (FR-R-147, FR-R-086) |
| RTU-over-stream derived extent above 256 bytes | oversized-ADU error, raised before it sizes any read (FR-R-149) |
| RTU-over-stream CRC mismatch | checksum error, and the delimitation is unsound with it: the failure is terminal for the stream, not frame-local (FR-R-150) |

Every row above is an error, never a panic: FR-R-130 admits no exception for any
input whatsoever.

## 2. Known limitations

- **ASCII terminators are strict.** Only CR LF is accepted. The Modbus serial
  specification permits a configurable end-of-frame character in some
  implementations; that configurability is not offered, so a peer using a
  non-standard terminator will not interoperate.
- **ASCII framing is provided at the frame layer only.** Whether a serial
  transport can be *operated* in ASCII mode — including that mode's 1-second
  inter-character timeout — is transport-area behavior and is not specified here.
  ASCII exists here so frames are readable in test fixtures and comparable
  against upstream tooling.
- **Broadcast is recognised, not enforced.** FR-R-096 names address 0; the rule
  that a server sends no response to a broadcast is server-area behavior.
- **RTU over a stream cannot carry every function code.** The boundary is derived
  from the frame's own length fields, and function code 8, function code 43
  outside MEI type 14, and every custom code have no derivable length
  (FR-R-148). They encode and decode perfectly well; what cannot be done is find
  where they end in a byte stream that gives no other clue. This is a property of
  the mode, not of this implementation: a transparent gateway forwards bytes and
  adds nothing to delimit them, so any stack reading them either derives the
  length as this one does, guesses, or scans for a CRC that matches by luck.
- **No Modbus Plus, and no serial-line ASCII delimiter negotiation.** Four
  framings only: RTU, RTU over stream, ASCII, TCP.
