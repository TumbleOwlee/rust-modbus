# Frame — Requirements

Normative behavior of the frame area: the Modbus PDU (function code plus body),
the three ADU framings (RTU with its CRC-16 trailer, ASCII with its LRC, TCP with
its MBAP header), request and response encoding, and exception responses.

This area is **role-agnostic**: it states what is true of a byte sequence
regardless of whether a client or a server produced it. Behavior that both roles
must share is specified here once, never twice.

IDs are stable and append-only (`FR-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (supported function
codes, exported types and error variants), [`data-contract.md`](./data-contract.md)
(ADU/PDU layouts, byte and word order, field widths),
[`edge-cases.md`](./edge-cases.md) (boundary and error behavior, stated
limitations).

---

## 1. PDU structure

**FR-R-001** — A PDU shall consist of a 1-byte function code followed by a
function-specific data body.

**FR-R-002** — A PDU shall be at most 253 bytes, inclusive of its function code.

**FR-R-003** — All multi-byte numeric fields in a PDU shall be encoded big-endian
(most significant byte first).

**FR-R-004** — A register value shall be a 16-bit unsigned quantity. The frame
layer shall not interpret registers as any wider or signed type; composition of
registers into other types is outside this area.

**FR-R-005** — Decoding a PDU shall require the caller to state whether the bytes
are a request or a response. A PDU is not self-describing: the same function code
carries different body layouts in each direction, and no decode shall infer the
direction from the bytes.

**FR-R-006** — Encoding a PDU that would exceed 253 bytes shall fail with a size
error rather than emit a truncated or oversized PDU.

---

## 2. Function code taxonomy

**FR-R-010** — The frame layer shall represent as named values exactly the
nineteen public function codes defined by the Modbus Application Protocol
specification: 1 Read Coils, 2 Read Discrete Inputs, 3 Read Holding Registers,
4 Read Input Registers, 5 Write Single Coil, 6 Write Single Register, 7 Read
Exception Status, 8 Diagnostics, 11 Get Comm Event Counter, 12 Get Comm Event
Log, 15 Write Multiple Coils, 16 Write Multiple Registers, 17 Report Server ID,
20 Read File Record, 21 Write File Record, 22 Mask Write Register, 23 Read/Write
Multiple Registers, 24 Read FIFO Queue, 43 Encapsulated Interface Transport.

**FR-R-011** — Any function code in the range 1–127 that is not one of the
nineteen in FR-R-010 shall be represented as a custom code carrying the raw byte.
This covers the user-defined ranges 65–72 and 100–110, the unassigned public
ranges, and any vendor code.

**FR-R-012** — A custom function code's body shall be treated as opaque: on
decode it shall comprise every remaining byte of the PDU, and on encode it shall
be emitted verbatim. The frame layer shall impose no structure on it beyond the
PDU size limit.

**FR-R-013** — Encoding a custom function code whose raw byte equals one of the
nineteen codes named in FR-R-010 shall fail with a reserved-code error. One wire
code shall have exactly one representation, so that decode and encode round-trip.

**FR-R-014** — Function code 0 shall be invalid in either direction and shall
fail to decode with an invalid-function-code error.

**FR-R-015** — Function codes 128–255 shall never denote a request. In the
response direction they denote an exception response per §7; in the request
direction they shall fail to decode with an invalid-function-code error.

---

## 3. Bit and register data access

**FR-R-020** — A read request PDU (function codes 1, 2, 3, 4) shall consist of a
2-byte starting address followed by a 2-byte quantity.

**FR-R-021** — The quantity in a Read Coils or Read Discrete Inputs request shall
be in the range 1–2000 inclusive; encoding a quantity outside that range shall
fail with an out-of-range error.

**FR-R-022** — The quantity in a Read Holding Registers or Read Input Registers
request shall be in the range 1–125 inclusive; encoding a quantity outside that
range shall fail with an out-of-range error.

**FR-R-023** — A bit-read response PDU (function codes 1, 2) shall consist of a
1-byte byte count followed by that many data bytes, where the byte count equals
`(quantity + 7) / 8`.

**FR-R-024** — In any bit-packed body, bit *n* of the range shall occupy bit
`n mod 8` of data byte `n / 8`, so the least significant bit of the first data
byte is the first coil or input. Unused high bits of the final byte shall be
encoded as zero.

**FR-R-025** — A register-read response PDU (function codes 3, 4) shall consist
of a 1-byte byte count followed by that many data bytes, where the byte count
equals `2 × quantity`.

**FR-R-026** — A Write Single Coil request PDU shall consist of a 2-byte output
address followed by a 2-byte value, where the value shall be `0xFF00` for ON and
`0x0000` for OFF, and no other value shall be encoded.

**FR-R-027** — Decoding a Write Single Coil request or response whose value is
neither `0xFF00` nor `0x0000` shall fail with an illegal-value error.

**FR-R-028** — A Write Single Register request PDU shall consist of a 2-byte
register address followed by the 2-byte value to write. Any 16-bit value shall be
permitted.

**FR-R-029** — The response to a Write Single Coil or Write Single Register
request shall be byte-for-byte identical to the request PDU.

**FR-R-030** — A Write Multiple Coils request PDU shall consist of a 2-byte
starting address, a 2-byte quantity, a 1-byte byte count, and that many data
bytes, where the byte count equals `(quantity + 7) / 8`.

**FR-R-031** — The quantity in a Write Multiple Coils request shall be in the
range 1–1968 inclusive; encoding a quantity outside that range shall fail with an
out-of-range error.

**FR-R-032** — A Write Multiple Registers request PDU shall consist of a 2-byte
starting address, a 2-byte quantity, a 1-byte byte count, and that many data
bytes, where the byte count equals `2 × quantity`.

**FR-R-033** — The quantity in a Write Multiple Registers request shall be in the
range 1–123 inclusive; encoding a quantity outside that range shall fail with an
out-of-range error.

**FR-R-034** — The response to a Write Multiple Coils or Write Multiple Registers
request shall consist of the 2-byte starting address followed by the 2-byte
quantity from the request.

**FR-R-035** — A Mask Write Register request PDU (function code 22) shall consist
of a 2-byte reference address, a 2-byte AND mask, and a 2-byte OR mask. Its
response shall be byte-for-byte identical to the request.

**FR-R-036** — The frame layer shall define the Mask Write Register result as
`(current AND and_mask) OR (or_mask AND (NOT and_mask))`, and shall expose that
computation. Applying it to stored data is the server area's behavior; defining
it is this area's.

**FR-R-037** — A Read/Write Multiple Registers request PDU (function code 23)
shall consist of a 2-byte read starting address, a 2-byte read quantity, a 2-byte
write starting address, a 2-byte write quantity, a 1-byte write byte count equal
to `2 × write quantity`, and that many data bytes. The read is performed after
the write.

**FR-R-038** — In a Read/Write Multiple Registers request the read quantity shall
be in the range 1–125 inclusive and the write quantity in the range 1–121
inclusive; encoding a quantity outside either range shall fail with an
out-of-range error.

**FR-R-039** — A Read/Write Multiple Registers response PDU shall consist of a
1-byte byte count equal to `2 × read quantity` followed by that many data bytes.

**FR-R-040** — A Read FIFO Queue request PDU (function code 24) shall consist of
a 2-byte FIFO pointer address.

**FR-R-041** — A Read FIFO Queue response PDU shall consist of a **2-byte** byte
count, a 2-byte FIFO count, and `2 × FIFO count` data bytes, where the byte count
equals `(2 × FIFO count) + 2`. The two-byte byte count is specific to this
function code and shall not be conflated with the one-byte byte count used
elsewhere.

**FR-R-042** — The FIFO count in a Read FIFO Queue response shall be at most 31;
decoding a greater count shall fail with an out-of-range error before any
allocation proportional to it is made.

**FR-R-043** — Decoding any PDU whose byte-count field disagrees with the number
of data bytes present, or with the value derived from its quantity field, shall
fail with a byte-count-mismatch error, and shall do so before any data bytes are
consumed.

**FR-R-044** — A bit-read response carries no bit count on the wire. Decoding one
shall yield exactly `8 × byte count` bit values, including the final byte's
padding bits. Matching the decoded bits against the quantity requested is the
caller's responsibility.

**FR-R-045** — A quantity outside the range its function code fixes shall fail to
decode as well as to encode. A PDU the encoder would reject shall not decode.

**FR-R-046** — A register-read response whose byte count is odd shall fail with an
illegal-value error naming the byte count, since no quantity of 16-bit registers
can produce one.

---

## 4. File record access

**FR-R-050** — A Read File Record request PDU (function code 20) shall consist of
a 1-byte request byte count followed by that many bytes of sub-requests, each
sub-request being exactly 7 bytes: a 1-byte reference type, a 2-byte file number,
a 2-byte record number, and a 2-byte record length in registers.

**FR-R-051** — The request byte count of a Read File Record request shall be in
the range 7–245 (`0x07`–`0xF5`) inclusive and shall be an exact multiple of 7; a
value violating either condition shall fail to decode with a byte-count error.

**FR-R-052** — A Read File Record response PDU shall consist of a 1-byte response
data length in the range 7–245 inclusive, followed by that many bytes of
sub-responses, each sub-response being a 1-byte file response length, a 1-byte
reference type, and record data; the file response length shall equal the record
data byte count plus one.

**FR-R-053** — A Write File Record request PDU (function code 21) shall consist
of a 1-byte request data length followed by that many bytes of sub-requests, each
being a 1-byte reference type, a 2-byte file number, a 2-byte record number, a
2-byte record length in registers, and `2 × record length` bytes of record data.
Its response shall be byte-for-byte identical to the request.

**FR-R-054** — The request data length of a Write File Record request shall be in
the range 9–251 (`0x09`–`0xFB`) inclusive; a value outside it shall fail to
decode with a byte-count error.

**FR-R-055** — The reference type in every file record sub-request and
sub-response shall be 6; decoding any other value shall fail with a
reference-type error.

**FR-R-056** — A file number shall be in the range 1–65535 and a record number in
the range 0–9999 (`0x270F`); encoding a value outside either range shall fail
with an out-of-range error.

---

## 5. Serial-line diagnostics

**FR-R-060** — A Read Exception Status request PDU (function code 7) shall consist
of the function code alone with no data body. Its response shall consist of a
single byte carrying eight exception status outputs.

**FR-R-061** — A Diagnostics request PDU (function code 8) shall consist of a
2-byte sub-function code followed by zero or more 16-bit data words. Its response
shall carry the same sub-function code followed by its own data words.

**FR-R-062** — The frame layer shall represent as named values exactly the
Diagnostics sub-functions defined by the specification: 0 Return Query Data,
1 Restart Communications Option, 2 Return Diagnostic Register, 3 Change ASCII
Input Delimiter, 4 Force Listen Only Mode, 10 Clear Counters and Diagnostic
Register, 11 Return Bus Message Count, 12 Return Bus Communication Error Count,
13 Return Bus Exception Error Count, 14 Return Server Message Count, 15 Return
Server No Response Count, 16 Return Server NAK Count, 17 Return Server Busy
Count, 18 Return Bus Character Overrun Count, 20 Clear Overrun Counter and Flag.

**FR-R-063** — Any Diagnostics sub-function code not named in FR-R-062, including
the reserved range 5–9, shall decode successfully into a general sub-function
value carrying the raw 16-bit code, and shall not fail the decode.

**FR-R-064** — A Get Comm Event Counter request PDU (function code 11) shall
consist of the function code alone. Its response shall consist of a 2-byte status
word followed by a 2-byte event count.

**FR-R-065** — A Get Comm Event Log request PDU (function code 12) shall consist
of the function code alone. Its response shall consist of a 1-byte byte count, a
2-byte status word, a 2-byte event count, a 2-byte message count, and 0–64 event
bytes, where the byte count equals the number of event bytes plus six.

**FR-R-066** — A Report Server ID request PDU (function code 17) shall consist of
the function code alone. Its response shall consist of a 1-byte byte count
followed by that many bytes: a device-specific server id of unspecified length, a
run indicator status, and additional device-specific data.

**FR-R-067** — The run indicator status in a Report Server ID response shall be
`0x00` for OFF and `0xFF` for ON; any other value shall fail to decode with an
illegal-value error.

**FR-R-068** — The status word in a Get Comm Event Counter or Get Comm Event Log
response shall be `0xFFFF` while the device is busy processing a program function
and `0x0000` otherwise. The frame layer shall carry the raw value without
rejecting others.

---

## 6. Encapsulated Interface Transport

**FR-R-070** — An Encapsulated Interface Transport PDU (function code 43) shall
begin with a 1-byte MEI type in both directions.

**FR-R-071** — The frame layer shall represent as named values exactly two MEI
types: 13 CANopen General Reference, and 14 Read Device Identification. Any other
MEI type shall decode into a general MEI value carrying the raw byte and an
opaque body comprising every remaining PDU byte.

**FR-R-072** — A CANopen General Reference PDU (MEI type 13) shall carry an
opaque body comprising every remaining PDU byte, emitted verbatim on encode. Its
contents are defined by CANopen and are outside this specification.

**FR-R-073** — A Read Device Identification request PDU (MEI type 14) shall
consist of the MEI type, a 1-byte read device id code, and a 1-byte object id.

**FR-R-074** — The read device id code shall be in the range 1–4 inclusive
(basic, regular, extended, individual); decoding any other value shall fail with
an out-of-range error.

**FR-R-075** — A Read Device Identification response PDU shall consist of the MEI
type, the read device id code, a 1-byte conformity level, a 1-byte more-follows
indicator, a 1-byte next object id, a 1-byte object count, and that many objects,
each being a 1-byte object id, a 1-byte object length, and that many object value
bytes.

**FR-R-076** — The more-follows indicator shall be `0x00` when no further object
follows and `0xFF` when the response is partial; any other value shall fail to
decode with an illegal-value error.

**FR-R-077** — Decoding a Read Device Identification response whose object count
does not match the number of complete objects present in the remaining bytes
shall fail with a byte-count-mismatch error.

---

## 7. Exception responses

**FR-R-080** — An exception response PDU shall consist of the request's function
code with its most significant bit set (`code | 0x80`) followed by a single
exception code byte.

**FR-R-081** — Decoding a response PDU whose function code has its most
significant bit set shall yield an exception response, not a normal response, and
shall report the original function code with the high bit cleared.

**FR-R-082** — The frame layer shall represent as named values exactly the nine
exception codes defined by the specification: 1 Illegal Function, 2 Illegal Data
Address, 3 Illegal Data Value, 4 Server Device Failure, 5 Acknowledge, 6 Server
Device Busy, 8 Memory Parity Error, 10 Gateway Path Unavailable, 11 Gateway
Target Device Failed To Respond.

**FR-R-083** — Any exception code not named in FR-R-082, including 0, shall decode
successfully into a general exception value carrying the raw byte, and shall not
fail the decode. A server's exception codes are its own to choose; a client that
cannot name one must still be able to report it.

**FR-R-084** — Encoding a general exception value whose raw byte equals one of the
nine codes named in FR-R-082 shall fail with a reserved-code error, on the same
round-trip grounds as FR-R-013.

**FR-R-085** — An exception response PDU with a length other than two bytes shall
fail to decode with a length error.

**FR-R-086** — An exception response shall be decodable for every function code,
including custom codes; the exception path shall not depend on the underlying
code being one of the nineteen public ones.

---

## 8. RTU ADU

**FR-R-090** — An RTU ADU shall consist of a 1-byte address field, the PDU, and a
2-byte CRC, in that order.

**FR-R-091** — An RTU ADU shall be at most 256 bytes.

**FR-R-092** — The RTU CRC shall be CRC-16 with the reversed polynomial `0xA001`
and an initial value of `0xFFFF`.

**FR-R-093** — The CRC shall be computed over every byte of the ADU preceding it —
the address field and the entire PDU — and over no other bytes.

**FR-R-094** — The CRC shall be transmitted low byte first, high byte second.

**FR-R-095** — Decoding an RTU ADU whose trailing CRC does not equal the CRC
computed over its preceding bytes shall fail with a checksum error, and the PDU
shall not be decoded.

**FR-R-096** — An RTU address field shall be an 8-bit value. Address 0 shall
denote a broadcast to all servers; addresses 1–247 shall denote individual
servers; addresses 248–255 shall decode without error and be left for the caller
to judge.

---

## 9. TCP ADU

**FR-R-100** — A TCP ADU shall consist of a 7-byte MBAP header followed by the
PDU.

**FR-R-101** — The MBAP header shall consist of a 2-byte transaction identifier,
a 2-byte protocol identifier, a 2-byte length field, and a 1-byte unit
identifier, each multi-byte field big-endian.

**FR-R-102** — The MBAP protocol identifier shall be encoded as 0; decoding a
header whose protocol identifier is non-zero shall fail with a
protocol-identifier error.

**FR-R-103** — The MBAP length field shall count the bytes following it — the unit
identifier plus the PDU — and shall therefore equal `PDU length + 1`.

**FR-R-104** — A TCP ADU shall be at most 260 bytes.

**FR-R-105** — Decoding a TCP ADU whose MBAP length field is 0, or exceeds 254,
shall fail with a length error before any allocation proportional to that field
is made.

**FR-R-106** — Decoding a TCP ADU shall fail with a length error if the MBAP
length field does not match the number of bytes actually supplied after it.

---

## 10. ASCII ADU

**FR-R-110** — An ASCII ADU shall consist of a start character `:` (`0x3A`), a
2-character address field, the PDU, a 2-character LRC, and a two-character
terminator CR LF (`0x0D 0x0A`), in that order.

**FR-R-111** — Every byte of the address field, the PDU, and the LRC shall be
transmitted as exactly two ASCII hexadecimal characters, most significant nibble
first.

**FR-R-112** — Encoding shall emit hexadecimal characters in uppercase (`0`–`9`,
`A`–`F`). Decoding shall accept both uppercase and lowercase; any character
outside `0`–`9`, `A`–`F`, `a`–`f` in a hexadecimal position shall fail with an
invalid-character error.

**FR-R-113** — An ASCII ADU shall be at most 513 characters, corresponding to 255
encoded bytes (1 address + 253 PDU + 1 LRC) plus the start character and the
two-character terminator.

**FR-R-114** — The LRC shall be the two's complement of the 8-bit sum of every
**decoded byte** preceding it — the address field and the entire PDU — and shall
be computed over the decoded bytes, never over their ASCII hexadecimal
characters.

**FR-R-115** — Decoding an ASCII ADU whose LRC does not equal the LRC computed
over its preceding decoded bytes shall fail with a checksum error, and the PDU
shall not be decoded.

**FR-R-116** — Decoding shall fail with a framing error if the ADU does not begin
with `:`, does not end with CR LF, or contains an odd number of hexadecimal
characters between them.

**FR-R-117** — The ASCII address field shall carry the same 8-bit value and the
same broadcast and range semantics as the RTU address field (FR-R-096).

**FR-R-118** — The frame layer shall support ASCII framing for both the request
and the response direction, and for every function code, custom codes and
exception responses included. ASCII is a framing choice, not a capability subset.

**FR-R-119** — An ASCII ADU shall re-encode to a byte sequence identical to its
input up to hexadecimal case: an ADU decoded from lowercase input re-encodes to
the uppercase form of the same ADU, which shall itself round-trip exactly.
FR-R-133 applies to the decoded PDU without qualification.

---

## 11. Robustness

**FR-R-130** — No decoding operation shall panic, index out of bounds, abort, or
allocate a quantity derived from unvalidated input, for any input byte sequence
whatsoever, including empty, truncated, oversized, and adversarially constructed
input.

**FR-R-131** — Decoding a PDU or ADU shorter than the layout its function code
requires shall fail with a truncated-input error naming the number of bytes
expected and the number supplied.

**FR-R-132** — Decoding a PDU that carries more bytes than its layout requires
shall fail with a trailing-bytes error rather than silently ignoring the surplus.
This shall not apply where the layout is defined as consuming all remaining
bytes: custom function codes (FR-R-012), CANopen and unknown MEI bodies
(FR-R-071, FR-R-072), and Report Server ID device data (FR-R-066).

**FR-R-133** — Every PDU the frame layer can decode shall re-encode to the
identical byte sequence. Decode and encode shall be inverse operations for all
valid input. For ASCII ADUs this holds subject to FR-R-119.
