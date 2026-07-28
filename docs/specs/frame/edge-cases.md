# Frame — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**.
Everything in §4 is working as specified; it is recorded here so it is not
mistaken for an oversight and silently "fixed".

---

## 1. PDU decode boundaries

| Condition | Behavior |
|---|---|
| Empty input | truncated-input error (FR-R-131) |
| PDU shorter than its layout requires | truncated-input error naming expected vs. supplied (FR-R-131) |
| PDU longer than its layout requires | trailing-bytes error (FR-R-132) |
| Function code 0 | invalid-function-code error (FR-R-014) |
| Function code 128–255 in the request direction | invalid-function-code error (FR-R-015) |
| Function code 1–127 that the crate does not name | decodes as `Custom(u8)`, body opaque (FR-R-011, FR-R-012) |
| Trailing bytes after a `Custom` body | none possible — the body is defined as all remaining bytes (FR-R-132) |
| Encoding `Custom(u8)` with a named code | reserved-code error (FR-R-013) |
| Byte count disagreeing with data length or with the quantity field | byte-count-mismatch error, raised before any data byte is consumed (FR-R-043) |
| Quantity outside its per-function range | out-of-range error, on decode as well as encode (FR-R-021, FR-R-022, FR-R-031, FR-R-033, FR-R-038, FR-R-045) |
| Register-read response with an odd byte count | illegal-value error naming the byte count (FR-R-046) |
| Bit-read response, coil count not on the wire | decodes to `8 × byte count` values, padding included (FR-R-044) |
| Write Single Coil value ∉ {`0x0000`, `0xFF00`} | illegal-value error (FR-R-027) |
| FC24 FIFO count > 31 | out-of-range error, raised before any sizing allocation (FR-R-042) |
| File record reference type ≠ 6 | reference-type error (FR-R-055) |
| FC20 request byte count not a multiple of 7 | byte-count error (FR-R-051) |
| Read device id code outside 1–4 | out-of-range error (FR-R-074) |
| More-follows indicator ∉ {`0x00`, `0xFF`} | illegal-value error (FR-R-076) |
| Run indicator status ∉ {`0x00`, `0xFF`} | illegal-value error (FR-R-067) |
| MEI 14 object count disagreeing with the objects present | byte-count-mismatch error (FR-R-077) |
| Unknown MEI type | decodes as a general MEI value with an opaque body (FR-R-071) |
| Unknown Diagnostics sub-function, including reserved 5–9 | decodes as a general sub-function value (FR-R-063) |
| Comm event status word ∉ {`0x0000`, `0xFFFF`} | carried as-is, no error (FR-R-068) |

## 2. Exception decode boundaries

| Condition | Behavior |
|---|---|
| Response function code with the high bit set | decoded as an exception response, not an error (FR-R-081) |
| Exception code outside the nine named, including 0 | decodes successfully as a general exception value (FR-R-083) |
| Encoding a general exception value holding a named code | reserved-code error (FR-R-084) |
| Exception response PDU of length ≠ 2 | length error (FR-R-085) |
| Exception response for a `Custom` function code | decodes normally; the exception path does not depend on the code being named (FR-R-086) |

## 3. ADU decode boundaries

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

Every row above is an error, never a panic: FR-R-130 admits no exception for any
input whatsoever.

## 4. Known limitations

- **Custom codes carry no semantics.** `Custom(u8)` preserves bytes; it does not
  know quantities, addresses, or lengths, so nothing beyond the PDU size limit is
  validated. A consumer using vendor codes owns their meaning.
- **CANopen (MEI 13) bodies are opaque.** CANopen is a separate specification and
  is not parsed.
- **Report Server ID bodies are opaque past the byte count.** The server id
  length is device-specific, so the split between id, run indicator, and
  additional data is not knowable to a generic decoder — the byte count is
  validated, the interior is not.
- **Diagnostics data words are not interpreted per sub-function.** The
  specification gives the data field different meanings per sub-function; the
  frame layer carries raw 16-bit words.
- **ASCII terminators are strict.** Only CR LF is accepted. The Modbus serial
  specification permits a configurable end-of-frame character in some
  implementations; that configurability is not offered, so a peer using a
  non-standard terminator will not interoperate.
- **ASCII framing is provided at the frame layer only.** Whether a serial
  transport can be *operated* in ASCII mode — including that mode's 1-second
  inter-character timeout — is transport-area behavior and is not specified here.
  ASCII exists here so frames are readable in test fixtures and comparable
  against upstream tooling.
- **The frame layer validates no address against any device map.** Structural
  validity only; Illegal Data Address is the server area's judgment.
- **Broadcast is recognised, not enforced.** FR-R-096 names address 0; the rule
  that a server sends no response to a broadcast is server-area behavior.
- **No Modbus Plus, no serial-line ASCII delimiter negotiation, no
  RTU-over-TCP gateway emulation.** Three framings only: RTU, ASCII, TCP.
