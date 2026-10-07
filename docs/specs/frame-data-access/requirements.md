# Frame Data Access — Requirements

Normative behavior of the data-carrying function codes: bit and register access, file record access, serial-line diagnostics, and Encapsulated Interface Transport (MEI). Split from [`../frame/`](../frame/), which retains PDU structure, the function code taxonomy, exception responses, robustness, buffer reuse, and serde/`Display` support.

This area is **role-agnostic**, on the same terms as `frame/`: it states what is true of a byte sequence regardless of who sent it.

IDs relocated from `frame/` keep their original `FR-R-nnn` numbers unchanged. Requirements added to this area after the split use `FR-DA-R-nnn`, stable and append-only. See [`../README.md`](../README.md).

Companion documents (shared with `frame/` and [`../frame-adu/`](../frame-adu/)): [`../frame/api-contract.md`](../frame/api-contract.md), [`../frame/data-contract.md`](../frame/data-contract.md). This area's own [`edge-cases.md`](./edge-cases.md) holds its boundary and error behavior.

---

## Bit and register data access

**FR-R-020** — A read request PDU (function codes 1, 2, 3, 4) consists of a 2-byte starting address followed by a 2-byte quantity.

**FR-R-021** — The quantity in a Read Coils or Read Discrete Inputs request is in the range 1–2000 inclusive; encoding a quantity outside that range fails with an out-of-range error.

**FR-R-022** — The quantity in a Read Holding Registers or Read Input Registers request is in the range 1–125 inclusive; encoding a quantity outside that range fails with an out-of-range error.

**FR-R-023** — A bit-read response PDU (function codes 1, 2) consists of a 1-byte byte count followed by that many data bytes, where the byte count equals `(quantity + 7) / 8`.

**FR-R-024** — In any bit-packed body, bit *n* of the range occupies bit `n mod 8` of data byte `n / 8`, so the least significant bit of the first data byte is the first coil or input. Unused high bits of the final byte are encoded as zero.

**FR-R-025** — A register-read response PDU (function codes 3, 4) consists of a 1-byte byte count followed by that many data bytes, where the byte count equals `2 × quantity`.

**FR-R-026** — A Write Single Coil request PDU consists of a 2-byte output address followed by a 2-byte value, where the value is `0xFF00` for ON and `0x0000` for OFF, and no other value is ever encoded.

**FR-R-027** — Decoding a Write Single Coil request or response whose value is neither `0xFF00` nor `0x0000` fails with an illegal-value error.

**FR-R-028** — A Write Single Register request PDU consists of a 2-byte register address followed by the 2-byte value to write. Any 16-bit value is permitted.

**FR-R-029** — The response to a Write Single Coil or Write Single Register request is byte-for-byte identical to the request PDU.

**FR-R-030** — A Write Multiple Coils request PDU consists of a 2-byte starting address, a 2-byte quantity, a 1-byte byte count, and that many data bytes, where the byte count equals `(quantity + 7) / 8`.

**FR-R-031** — The quantity in a Write Multiple Coils request is in the range 1–1968 inclusive; encoding a quantity outside that range fails with an out-of-range error.

**FR-R-032** — A Write Multiple Registers request PDU consists of a 2-byte starting address, a 2-byte quantity, a 1-byte byte count, and that many data bytes, where the byte count equals `2 × quantity`.

**FR-R-033** — The quantity in a Write Multiple Registers request is in the range 1–123 inclusive; encoding a quantity outside that range fails with an out-of-range error.

**FR-R-034** — The response to a Write Multiple Coils or Write Multiple Registers request consists of the 2-byte starting address followed by the 2-byte quantity from the request.

**FR-R-035** — A Mask Write Register request PDU (function code 22) consists of a 2-byte reference address, a 2-byte AND mask, and a 2-byte OR mask.

**FR-DA-R-001** — The response to a Mask Write Register request (FR-R-035) is byte-for-byte identical to the request.

**FR-R-036** — The frame layer defines the Mask Write Register result as `(current AND and_mask) OR (or_mask AND (NOT and_mask))`, and exposes that computation. Applying it to stored data is the server area's behavior; defining it is this area's.

**FR-R-037** — A Read/Write Multiple Registers request PDU (function code 23) consists of a 2-byte read starting address, a 2-byte read quantity, a 2-byte write starting address, a 2-byte write quantity, a 1-byte write byte count equal to `2 × write quantity`, and that many data bytes. The read is performed after the write.

**FR-R-038** — In a Read/Write Multiple Registers request the read quantity is in the range 1–125 inclusive and the write quantity in the range 1–121 inclusive; encoding a quantity outside either range fails with an out-of-range error.

**FR-R-039** — A Read/Write Multiple Registers response PDU consists of a 1-byte byte count equal to `2 × read quantity` followed by that many data bytes.

**FR-R-040** — A Read FIFO Queue request PDU (function code 24) consists of a 2-byte FIFO pointer address.

**FR-R-041** — A Read FIFO Queue response PDU consists of a **2-byte** byte count, a 2-byte FIFO count, and `2 × FIFO count` data bytes, where the byte count equals `(2 × FIFO count) + 2`. The two-byte byte count is specific to this function code and is never conflated with the one-byte byte count used elsewhere.

**FR-R-042** — The FIFO count in a Read FIFO Queue response is at most 31; decoding a greater count fails with an out-of-range error before any allocation proportional to it is made.

**FR-R-043** — Decoding any PDU whose byte-count field disagrees with the number of data bytes present, or with the value derived from its quantity field, fails with a byte-count-mismatch error, and does so before any data bytes are consumed.

**FR-R-044** — A bit-read response carries no bit count on the wire. Decoding one yields exactly `8 × byte count` bit values, including the final byte's padding bits. Matching the decoded bits against the quantity requested is the caller's responsibility.

**FR-R-045** — A quantity outside the range its function code fixes fails to decode as well as to encode. A PDU the encoder would reject does not decode.

**FR-R-046** — A register-read response whose byte count is odd fails with an illegal-value error naming the byte count, since no quantity of 16-bit registers can produce one.

**FR-R-047** — Decoding a bit-packed request body whose padding bits above the stated quantity are not zero fails with an illegal-value error naming the offending byte.

---

## File record access

**FR-R-050** — A Read File Record request PDU (function code 20) consists of a 1-byte request byte count followed by that many bytes of sub-requests, each sub-request being exactly 7 bytes: a 1-byte reference type, a 2-byte file number, a 2-byte record number, and a 2-byte record length in registers.

**FR-R-051** — The request byte count of a Read File Record request is in the range 7–245 (`0x07`–`0xF5`) inclusive; a value outside the range fails to decode with an out-of-range error.

**FR-DA-R-002** — The request byte count of a Read File Record request is an exact multiple of 7; a value inside the FR-R-051 range that is not a multiple of 7 fails to decode with an illegal-value error naming the byte count.

**FR-R-052** — A Read File Record response PDU consists of a 1-byte response data length in the range 7–245 inclusive, followed by that many bytes of sub-responses, each sub-response being a 1-byte file response length, a 1-byte reference type, and record data; the file response length equals the record data byte count plus one.

**FR-R-053** — A Write File Record request PDU (function code 21) consists of a 1-byte request data length followed by that many bytes of sub-requests, each being a 1-byte reference type, a 2-byte file number, a 2-byte record number, a 2-byte record length in registers, and `2 × record length` bytes of record data.

**FR-DA-R-003** — The response to a Write File Record request (FR-R-053) is byte-for-byte identical to the request.

**FR-R-054** — The request data length of a Write File Record request is in the range 9–251 (`0x09`–`0xFB`) inclusive; a value outside it fails to decode with a byte-count error.

**FR-R-055** — The reference type in every file record sub-request and sub-response is 6; decoding any other value fails with a reference-type error.

**FR-R-056** — A file number is in the range 1–65535 and a record number in the range 0–9999 (`0x270F`); encoding a value outside either range fails with an out-of-range error.

**FR-R-057** — A file response length that is zero or even fails to decode with an illegal-value error naming the field, since it claims an odd number of record data bytes and cannot hold whole registers.

**FR-R-058** — The file number and record number ranges FR-R-056 fixes are enforced on decode as well as on encode.

---

## Serial-line diagnostics

**FR-R-060** — A Read Exception Status request PDU (function code 7) consists of the function code alone with no data body. Its response consists of a single byte carrying eight exception status outputs.

**FR-R-061** — A Diagnostics request PDU (function code 8) consists of a 2-byte sub-function code followed by zero or more 16-bit data words. Its response carries the same sub-function code followed by its own data words.

**FR-DA-R-004** — Decoding a Diagnostics body (FR-R-061) whose bytes after the sub-function code do not divide into whole 16-bit words fails with an illegal-value error naming the data length.

**FR-R-062** — The frame layer represents as named values exactly the Diagnostics sub-functions defined by the specification: 0 Return Query Data, 1 Restart Communications Option, 2 Return Diagnostic Register, 3 Change ASCII Input Delimiter, 4 Force Listen Only Mode, 10 Clear Counters and Diagnostic Register, 11 Return Bus Message Count, 12 Return Bus Communication Error Count, 13 Return Bus Exception Error Count, 14 Return Server Message Count, 15 Return Server No Response Count, 16 Return Server NAK Count, 17 Return Server Busy Count, 18 Return Bus Character Overrun Count, 20 Clear Overrun Counter and Flag.

**FR-R-063** — Any Diagnostics sub-function code not named in FR-R-062, including the reserved range 5–9, decodes successfully into a general sub-function value carrying the raw 16-bit code, and never fails the decode.

**FR-DA-R-005** — Encoding a general Diagnostics sub-function value (FR-R-063) that holds a code FR-R-062 names fails with a reserved-code error, so no sub-function has two encodings.

**FR-R-064** — A Get Comm Event Counter request PDU (function code 11) consists of the function code alone. Its response consists of a 2-byte status word followed by a 2-byte event count.

**FR-R-065** — A Get Comm Event Log request PDU (function code 12) consists of the function code alone. Its response consists of a 1-byte byte count, a 2-byte status word, a 2-byte event count, a 2-byte message count, and 0–64 event bytes, where the byte count equals the number of event bytes plus six.

**FR-DA-R-006** — The byte count of a Get Comm Event Log response (FR-R-065) is in the range 6–70 inclusive; a value outside it fails to decode with an out-of-range error, raised before any event byte is consumed.

**FR-R-066** — A Report Server ID request PDU (function code 17) consists of the function code alone. Its response consists of a 1-byte byte count followed by that many bytes: a device-specific server id of unspecified length, a run indicator status, and additional device-specific data.

**FR-R-067** — The frame layer carries a Report Server ID response body whole, without interpreting the server id, run indicator, or additional data within it. The run indicator's `0x00`/`0xFF` encoding is the responsibility of the server that constructs the body (`SV-R-*`).

**FR-R-068** — The status word in a Get Comm Event Counter or Get Comm Event Log response is `0xFFFF` while the device is busy processing a program function and `0x0000` otherwise. The frame layer carries the raw value without rejecting others.

---

## Encapsulated Interface Transport

**FR-R-070** — An Encapsulated Interface Transport PDU (function code 43) begins with a 1-byte MEI type in both directions.

**FR-R-071** — The frame layer represents as named values exactly two MEI types: 13 CANopen General Reference, and 14 Read Device Identification. Any other MEI type decodes into a general MEI value carrying the raw byte and an opaque body comprising every remaining PDU byte.

**FR-R-072** — A CANopen General Reference PDU (MEI type 13) carries an opaque body comprising every remaining PDU byte, emitted verbatim on encode. Its contents are defined by CANopen and are outside this specification.

**FR-R-073** — A Read Device Identification request PDU (MEI type 14) consists of the MEI type, a 1-byte read device id code, and a 1-byte object id.

**FR-R-074** — The read device id code is in the range 1–4 inclusive (basic, regular, extended, individual); decoding any other value fails with an out-of-range error.

**FR-R-075** — A Read Device Identification response PDU consists of the MEI type, the read device id code, a 1-byte conformity level, a 1-byte more-follows indicator, a 1-byte next object id, a 1-byte object count, and that many objects, each being a 1-byte object id, a 1-byte object length, and that many object value bytes.

**FR-R-076** — The more-follows indicator is `0x00` when no further object follows and `0xFF` when the response is partial; any other value fails to decode with an illegal-value error.

**FR-R-077** — Decoding a Read Device Identification response whose object count does not match the number of complete objects present in the remaining bytes fails with a byte-count-mismatch error.

**FR-R-078** — Encoding a Read Device Identification response with more than 255 objects, or an object whose value exceeds 255 bytes, fails with an out-of-range error naming the field. Both are 1-byte fields on the wire.
