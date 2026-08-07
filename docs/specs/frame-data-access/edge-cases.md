# Frame Data Access — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**,
for bit/register access, file record access, serial-line diagnostics, and MEI.
Core PDU/exception/buffer edge cases live in
[`../frame/edge-cases.md`](../frame/edge-cases.md); ADU edge cases live in
[`../frame-adu/edge-cases.md`](../frame-adu/edge-cases.md).

Everything in §2 is working as specified; it is recorded here so it is not
mistaken for an oversight and silently "fixed".

---

## 1. PDU decode boundaries

| Condition | Behavior |
|---|---|
| Byte count disagreeing with data length or with the quantity field | byte-count-mismatch error, raised before any data byte is consumed (FR-R-043) |
| Quantity outside its per-function range | out-of-range error, on decode as well as encode (FR-R-021, FR-R-022, FR-R-031, FR-R-033, FR-R-038, FR-R-045) |
| Register-read response with an odd byte count | illegal-value error naming the byte count (FR-R-046) |
| Bit-read response, coil count not on the wire | decodes to `8 × byte count` values, padding included (FR-R-044) |
| Bit-packed request body with non-zero padding above the quantity | illegal-value error naming the byte (FR-R-047) |
| Write Single Coil value ∉ {`0x0000`, `0xFF00`} | illegal-value error (FR-R-027) |
| FC24 FIFO count > 31 | out-of-range error, raised before any sizing allocation (FR-R-042) |
| File record reference type ≠ 6 | reference-type error (FR-R-055) |
| FC20 request byte count outside 7–245 | out-of-range error (FR-R-051) |
| FC20 request byte count not a multiple of 7 | illegal-value error naming the byte count (FR-R-051) |
| File response length zero or even | illegal-value error naming the field (FR-R-057) |
| File or record number outside its range | out-of-range error, on decode as well as encode (FR-R-056, FR-R-058) |
| File record sub-item overrunning its length-delimited region | truncated-input error measured against the stated length (FR-R-131) |
| Read device id code outside 1–4 | out-of-range error (FR-R-074) |
| More-follows indicator ∉ {`0x00`, `0xFF`} | illegal-value error (FR-R-076) |
| Run indicator status ∉ {`0x00`, `0xFF`} | carried as-is; the frame layer does not locate it (FR-R-067) |
| FC17 byte count of 0 | decodes to an empty body; no minimum is specified (FR-R-066) |
| FC8 body not dividing into whole 16-bit words | illegal-value error naming the data length (FR-R-061) |
| FC12 byte count outside 6–70 | out-of-range error, raised before any event byte is consumed (FR-R-065) |
| Encoding a general Diagnostics sub-function holding a named code | reserved-code error (FR-R-063) |
| MEI 14 object count disagreeing with the objects present | byte-count-mismatch error, its two numbers counting objects rather than bytes (FR-R-077) |
| MEI 14 response whose final object is cut short mid-value | truncated-input error, not a count mismatch (FR-R-131) |
| MEI 14 response with 256+ objects, or an object value over 255 bytes | out-of-range error on encode (FR-R-078) |
| Unknown MEI type | decodes as a general MEI value with an opaque body (FR-R-071) |
| Unknown Diagnostics sub-function, including reserved 5–9 | decodes as a general sub-function value (FR-R-063) |
| Comm event status word ∉ {`0x0000`, `0xFFFF`} | carried as-is, no error (FR-R-068) |

Every row above is an error, never a panic: FR-R-130 admits no exception for any
input whatsoever.

## 2. Known limitations

- **Bit padding is treated differently in requests and responses, on purpose.**
  A bit-read *response* keeps all `8 × byte count` values including padding
  (FR-R-044); a bit-write *request* truncates to its quantity and rejects
  non-zero padding (FR-R-047). The asymmetry is not an oversight: the response
  carries no quantity field, so its padding bits are indistinguishable from real
  coil values, while the request's quantity says exactly which bits are padding.
  Both rules exist to keep decode and encode inverse (FR-R-133). Do not "fix"
  this into false symmetry.
- **A FC20 sub-request's record length is not range-checked.** The
  specification fixes no bound for it, and the size of the response it provokes
  is the server's to judge, so only the 2-byte field width limits it. A request
  asking for more registers than a response could carry decodes fine; the
  server answers with an exception.
- **CANopen (MEI 13) bodies are opaque.** CANopen is a separate specification and
  is not parsed.
- **Report Server ID bodies are opaque past the byte count.** The server id
  length is device-specific, so the split between id, run indicator, and
  additional data is not knowable to a generic decoder — the byte count is
  validated, the interior is not. This is why FR-R-067 places the run
  indicator's encoding with the server rather than here: nothing at this layer
  can locate the byte to check it.
- **Diagnostics data words are not interpreted per sub-function.** The
  specification gives the data field different meanings per sub-function; the
  frame layer carries raw 16-bit words.
