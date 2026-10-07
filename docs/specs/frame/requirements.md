# Frame — Requirements

Normative behavior of the frame area's core: PDU structure, the function code taxonomy, exception responses, robustness, buffer reuse, and serde/`Display` support. Shared by every framing and every function-code group.

This area is **role-agnostic**: it states what is true of a byte sequence regardless of whether a client or a server produced it. Behavior that both roles share is specified here once, never twice.

IDs are stable and append-only (`FR-R-nnn`). See [`../README.md`](../README.md).

Sub-areas split from this one, sharing this area's `api-contract.md` and `data-contract.md`: [`../frame-data-access/`](../frame-data-access/) (bit/register, file record, diagnostics, MEI — `FR-DA-R-nnn`), [`../frame-adu/`](../frame-adu/) (RTU, TCP, ASCII, RTU-over-stream, the framing abstraction — `FR-ADU-R-nnn`).

Companion documents: [`api-contract.md`](./api-contract.md) (supported function codes, exported types and error variants — shared across all three frame sub-areas), [`data-contract.md`](./data-contract.md) (ADU/PDU layouts, byte and word order, field widths — shared), [`edge-cases.md`](./edge-cases.md) (this sub-area's boundary and error behavior, stated limitations).

---

## PDU structure

**FR-R-001** — A PDU consists of a 1-byte function code followed by a function-specific data body.

**FR-R-002** — A PDU is at most 253 bytes, inclusive of its function code.

**FR-R-003** — All multi-byte numeric fields in a PDU are encoded big-endian (most significant byte first).

**FR-R-004** — A register value is a 16-bit unsigned quantity. The frame layer never interprets registers as any wider or signed type; composition of registers into other types is outside this area.

**FR-R-005** — Decoding a PDU requires the caller to state whether the bytes are a request or a response. A PDU is not self-describing: the same function code carries different body layouts in each direction, and no decode infers the direction from the bytes.

**FR-R-006** — Encoding a PDU that would exceed 253 bytes fails with a size error rather than emit a truncated or oversized PDU.

**FR-R-007** — The frame layer represents each domain value it carries — unit identifier, transaction identifier, data address, quantity, register value, mask, file number, record number, record length, exception status — as a distinct type, so that two values of different meaning cannot be interchanged.

**FR-R-155** — Each domain value type of FR-R-007 is a transparent wrapper over its wire representation and imposes no validation beyond that representation's width; a value that is legal on the wire is constructible.

**FR-R-160** — Every 16-bit value 0–65535 is a valid start address in each of the four data tables (coils, discrete inputs, input registers, holding registers); encoding and decoding reject none of them.

---

## Function code taxonomy

**FR-R-010** — The frame layer represents as named values exactly the nineteen public function codes defined by the Modbus Application Protocol specification: 1 Read Coils, 2 Read Discrete Inputs, 3 Read Holding Registers, 4 Read Input Registers, 5 Write Single Coil, 6 Write Single Register, 7 Read Exception Status, 8 Diagnostics, 11 Get Comm Event Counter, 12 Get Comm Event Log, 15 Write Multiple Coils, 16 Write Multiple Registers, 17 Report Server ID, 20 Read File Record, 21 Write File Record, 22 Mask Write Register, 23 Read/Write Multiple Registers, 24 Read FIFO Queue, 43 Encapsulated Interface Transport.

**FR-R-011** — Any function code in the range 1–127 that is not one of the nineteen in FR-R-010 is represented as a custom code carrying the raw byte. This covers the user-defined ranges 65–72 and 100–110, the unassigned public ranges, and any vendor code.

**FR-R-012** — A custom function code's body is opaque: on decode it comprises every remaining byte of the PDU, and on encode it is emitted verbatim. The frame layer imposes no structure on it beyond the PDU size limit.

**FR-R-013** — Encoding a custom function code whose raw byte equals one of the nineteen codes named in FR-R-010 fails with a reserved-code error. One wire code has exactly one representation, so that decode and encode round-trip.

**FR-R-014** — Function code 0 is invalid in either direction and fails to decode with an invalid-function-code error.

**FR-R-015** — Function codes 128–255 never denote a request. In the response direction they denote an exception response per FR-R-081; in the request direction they fail to decode with an invalid-function-code error.

**FR-R-016** — A request or response PDU reports the function code it carries, including the code an exception response is an exception to. A decoded PDU answers this without being re-encoded.

---

## Exception responses

**FR-R-080** — An exception response PDU consists of the request's function code with its most significant bit set (`code | 0x80`) followed by a single exception code byte.

**FR-R-081** — Decoding a response PDU whose function code has its most significant bit set yields an exception response, not a normal response, and reports the original function code with the high bit cleared.

**FR-R-082** — The frame layer represents as named values exactly the nine exception codes defined by the specification: 1 Illegal Function, 2 Illegal Data Address, 3 Illegal Data Value, 4 Server Device Failure, 5 Acknowledge, 6 Server Device Busy, 8 Memory Parity Error, 10 Gateway Path Unavailable, 11 Gateway Target Device Failed To Respond.

**FR-R-083** — Any exception code not named in FR-R-082, including 0, decodes successfully into a general exception value carrying the raw byte, and never fails the decode. A server's exception codes are its own to choose; a client that cannot name one is still able to report it.

**FR-R-084** — Encoding a general exception value whose raw byte equals one of the nine codes named in FR-R-082 fails with a reserved-code error, on the same round-trip grounds as FR-R-013.

**FR-R-085** — An exception response PDU with a length other than two bytes fails to decode with a length error.

**FR-R-086** — An exception response is decodable for every function code, including custom codes; the exception path does not depend on the underlying code being one of the nineteen public ones.

---

## Robustness

**FR-R-130** — No decoding operation panics, indexes out of bounds, aborts, or allocates a quantity derived from unvalidated input, for any input byte sequence whatsoever, including empty, truncated, oversized, and adversarially constructed input.

**FR-R-131** — Decoding a PDU or ADU shorter than the layout its function code requires fails with a truncated-input error naming the number of bytes expected and the number supplied.

**FR-R-132** — Decoding a PDU that carries more bytes than its layout requires fails with a trailing-bytes error rather than silently ignoring the surplus. This does not apply where the layout is defined as consuming all remaining bytes: custom function codes (FR-R-012), CANopen and unknown MEI bodies (`FR-DA-R-*`), and Report Server ID device data (`FR-DA-R-*`).

**FR-R-133** — Every PDU the frame layer can decode re-encodes to the identical byte sequence. Decode and encode are inverse operations for all valid input. For ASCII ADUs this holds subject to `FR-ADU-R-*`'s hexadecimal-case rule.

**FR-R-159** — A decode failure that no more specific error variant describes is reported as `Error::Malformed`.

---

## Buffer reuse

**FR-R-140** — The frame layer offers, for both directions and at both the PDU and the ADU level, encoding that *appends* to a caller-supplied buffer alongside the existing encoding that returns a new one.

**FR-R-156** — The appending encode of FR-R-140 is the primitive and the allocating form is defined in terms of it, so the two never describe different bytes.

**FR-R-141** — Appending encode reserves the capacity it needs before it writes the first byte. An ADU encode reserves its framing's maximum ADU length (`FR-ADU-R-*`), which bounds every PDU it can carry, so that no encode below it reallocates the caller's buffer. A caller that reuses one buffer across frames therefore allocates at most once.

**FR-R-142** — An appending encode that fails leaves the caller's buffer exactly as it found it, truncated back to its length on entry. A caller that reuses a buffer after a failure never transmits a fragment of an abandoned frame.

**FR-R-143** — Appending encode allocates no intermediate buffer per frame, except in ASCII framing, whose wire form is a character transformation of the binary ADU (`FR-ADU-R-*`) rather than a wrapping of it, and which may use one scratch buffer per frame.

**FR-R-144** — Each ADU boundary rule states whether it is **self-locating**: whether the next frame boundary can be found from the wire alone, without reference to the frame before it. A boundary determined by inter-frame silence or by delimiters is self-locating. A boundary determined by a length field, or derived from the ADU's own content, is not, since in both cases the information that would delimit the next frame is carried by the frame that failed. This property is derived from the boundary rule itself, so that a framing cannot state one rule and behave by another.

---

## Serde support and Display

**FR-R-151** — Behind the crate's `serde` feature, each domain value type of FR-R-007 implements `serde::Serialize` and `serde::Deserialize` as `#[serde(transparent)]`: the wrapped integer serializes and deserializes with no wrapping structure of its own, so a `UnitId(17)` field serializes identically to a bare `17`.

**FR-R-157** — Deserialization of a domain value type (FR-R-151) imposes no validation beyond the wrapped integer's own width, on the same terms FR-R-155 states for construction: a value that deserializes is always constructible, including one no function code would accept.

**FR-R-152** — Every domain value type of FR-R-007 implements `core::fmt::Display`, rendering exactly the wrapped value with no type name, no field name and no surrounding punctuation, so that `format!("unit {unit}")` composes without the caller stripping a wrapper. This is unconditional, not gated by any feature.

**FR-R-158** — `Debug` of a domain value type of FR-R-007 is unaffected by its `Display` (FR-R-152) and shows the wrapper.

**FR-R-153** — `FunctionCode` implements `core::fmt::Display`, unconditionally. A named code renders as the English name FR-R-010 gives it, spelled exactly as FR-R-010 spells it — including `Read/Write Multiple Registers` and `Read FIFO Queue`. `Custom(u8)` renders as `"Custom function "` followed by the decimal byte value.

**FR-R-154** — `ExceptionCode` implements `core::fmt::Display`, unconditionally. A named code renders as the English name FR-R-082 gives it, spelled exactly as FR-R-082 spells it. `Other(u8)` renders as `"Other exception "` followed by the decimal byte value.
