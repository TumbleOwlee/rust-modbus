# Frame — Requirements

Normative behavior of the frame area's core: PDU structure, the function code
taxonomy, exception responses, robustness, buffer reuse, and serde/`Display`
support. Shared by every framing and every function-code group.

This area is **role-agnostic**: it states what is true of a byte sequence
regardless of whether a client or a server produced it. Behavior that both roles
must share is specified here once, never twice.

IDs are stable and append-only (`FR-R-nnn`). See [`../README.md`](../README.md).

Sub-areas split from this one, sharing this area's `api-contract.md` and
`data-contract.md`: [`../frame-data-access/`](../frame-data-access/)
(bit/register, file record, diagnostics, MEI — `FR-DA-R-nnn`),
[`../frame-adu/`](../frame-adu/) (RTU, TCP, ASCII, RTU-over-stream, the framing
abstraction — `FR-ADU-R-nnn`).

Companion documents: [`api-contract.md`](./api-contract.md) (supported function
codes, exported types and error variants — shared across all three frame
sub-areas), [`data-contract.md`](./data-contract.md) (ADU/PDU layouts, byte and
word order, field widths — shared), [`edge-cases.md`](./edge-cases.md) (this
sub-area's boundary and error behavior, stated limitations).

---

## 1. PDU structure

**FR-R-001** — A PDU shall consist of a 1-byte function code followed by a function-specific data body.

**FR-R-002** — A PDU shall be at most 253 bytes, inclusive of its function code.

**FR-R-003** — All multi-byte numeric fields in a PDU shall be encoded big-endian (most significant byte first).

**FR-R-004** — A register value shall be a 16-bit unsigned quantity. The frame layer shall not interpret registers as any wider or signed type; composition of registers into other types is outside this area.

**FR-R-005** — Decoding a PDU shall require the caller to state whether the bytes are a request or a response. A PDU is not self-describing: the same function code carries different body layouts in each direction, and no decode shall infer the direction from the bytes.

**FR-R-006** — Encoding a PDU that would exceed 253 bytes shall fail with a size error rather than emit a truncated or oversized PDU.

**FR-R-007** — The frame layer shall represent each domain value it carries — unit identifier, transaction identifier, data address, quantity, register value, mask, file number, record number, record length, exception status — as a distinct type, so that two values of different meaning cannot be interchanged. Each shall be a transparent wrapper over its wire representation and shall impose no validation beyond that representation's width; a value that is legal on the wire shall be constructible.

---

## 2. Function code taxonomy

**FR-R-010** — The frame layer shall represent as named values exactly the nineteen public function codes defined by the Modbus Application Protocol specification: 1 Read Coils, 2 Read Discrete Inputs, 3 Read Holding Registers, 4 Read Input Registers, 5 Write Single Coil, 6 Write Single Register, 7 Read Exception Status, 8 Diagnostics, 11 Get Comm Event Counter, 12 Get Comm Event Log, 15 Write Multiple Coils, 16 Write Multiple Registers, 17 Report Server ID, 20 Read File Record, 21 Write File Record, 22 Mask Write Register, 23 Read/Write Multiple Registers, 24 Read FIFO Queue, 43 Encapsulated Interface Transport.

**FR-R-011** — Any function code in the range 1–127 that is not one of the nineteen in FR-R-010 shall be represented as a custom code carrying the raw byte. This covers the user-defined ranges 65–72 and 100–110, the unassigned public ranges, and any vendor code.

**FR-R-012** — A custom function code's body shall be treated as opaque: on decode it shall comprise every remaining byte of the PDU, and on encode it shall be emitted verbatim. The frame layer shall impose no structure on it beyond the PDU size limit.

**FR-R-013** — Encoding a custom function code whose raw byte equals one of the nineteen codes named in FR-R-010 shall fail with a reserved-code error. One wire code shall have exactly one representation, so that decode and encode round-trip.

**FR-R-014** — Function code 0 shall be invalid in either direction and shall fail to decode with an invalid-function-code error.

**FR-R-015** — Function codes 128–255 shall never denote a request. In the response direction they denote an exception response per §3; in the request direction they shall fail to decode with an invalid-function-code error.

**FR-R-016** — A request or response PDU shall report the function code it carries, including the code an exception response is an exception to. A decoded PDU shall answer this without being re-encoded.

---

## 3. Exception responses

**FR-R-080** — An exception response PDU shall consist of the request's function code with its most significant bit set (`code | 0x80`) followed by a single exception code byte.

**FR-R-081** — Decoding a response PDU whose function code has its most significant bit set shall yield an exception response, not a normal response, and shall report the original function code with the high bit cleared.

**FR-R-082** — The frame layer shall represent as named values exactly the nine exception codes defined by the specification: 1 Illegal Function, 2 Illegal Data Address, 3 Illegal Data Value, 4 Server Device Failure, 5 Acknowledge, 6 Server Device Busy, 8 Memory Parity Error, 10 Gateway Path Unavailable, 11 Gateway Target Device Failed To Respond.

**FR-R-083** — Any exception code not named in FR-R-082, including 0, shall decode successfully into a general exception value carrying the raw byte, and shall not fail the decode. A server's exception codes are its own to choose; a client that cannot name one must still be able to report it.

**FR-R-084** — Encoding a general exception value whose raw byte equals one of the nine codes named in FR-R-082 shall fail with a reserved-code error, on the same round-trip grounds as FR-R-013.

**FR-R-085** — An exception response PDU with a length other than two bytes shall fail to decode with a length error.

**FR-R-086** — An exception response shall be decodable for every function code, including custom codes; the exception path shall not depend on the underlying code being one of the nineteen public ones.

---

## 4. Robustness

**FR-R-130** — No decoding operation shall panic, index out of bounds, abort, or allocate a quantity derived from unvalidated input, for any input byte sequence whatsoever, including empty, truncated, oversized, and adversarially constructed input.

**FR-R-131** — Decoding a PDU or ADU shorter than the layout its function code requires shall fail with a truncated-input error naming the number of bytes expected and the number supplied.

**FR-R-132** — Decoding a PDU that carries more bytes than its layout requires shall fail with a trailing-bytes error rather than silently ignoring the surplus. This shall not apply where the layout is defined as consuming all remaining bytes: custom function codes (FR-R-012), CANopen and unknown MEI bodies (`FR-DA-R-*`), and Report Server ID device data (`FR-DA-R-*`).

**FR-R-133** — Every PDU the frame layer can decode shall re-encode to the identical byte sequence. Decode and encode shall be inverse operations for all valid input. For ASCII ADUs this holds subject to `FR-ADU-R-*`'s hexadecimal-case rule.

---

## 5. Buffer reuse

**FR-R-140** — The frame layer shall offer, for both directions and at both the PDU and the ADU level, encoding that *appends* to a caller-supplied buffer alongside the existing encoding that returns a new one. The appending form shall be the primitive and the allocating form shall be defined in terms of it, so the two can never describe different bytes.

**FR-R-141** — Appending encode shall reserve the capacity it needs before it writes the first byte. An ADU encode shall reserve its framing's maximum ADU length (`FR-ADU-R-*`), which bounds every PDU it can carry, so that no encode below it reallocates the caller's buffer. A caller that reuses one buffer across frames shall therefore allocate at most once.

**FR-R-142** — An appending encode that fails shall leave the caller's buffer exactly as it found it, truncated back to its length on entry. A caller that reuses a buffer after a failure shall never transmit a fragment of an abandoned frame.

**FR-R-143** — Appending encode shall allocate no intermediate buffer per frame, except in ASCII framing, whose wire form is a character transformation of the binary ADU (`FR-ADU-R-*`) rather than a wrapping of it, and which may use one scratch buffer per frame.

**FR-R-144** — Each ADU boundary rule shall state whether it is **self-locating**: whether the next frame boundary can be found from the wire alone, without reference to the frame before it. A boundary determined by inter-frame silence or by delimiters shall be self-locating. A boundary determined by a length field, or derived from the ADU's own content, shall not be, since in both cases the information that would delimit the next frame is carried by the frame that failed. This property shall be derived from the boundary rule itself, so that a framing cannot state one rule and behave by another.

---

## 6. Serde support and Display

**FR-R-151** — Behind the crate's `serde` feature, each domain value type of FR-R-007 shall implement `serde::Serialize` and `serde::Deserialize` as `#[serde(transparent)]`: the wrapped integer serializes and deserializes with no wrapping structure of its own, so a `UnitId(17)` field serializes identically to a bare `17`. Deserialization shall impose no validation beyond the wrapped integer's own width, on the same terms FR-R-007 already states for construction: a value that deserializes is always constructible, including one no function code would accept.

**FR-R-152** — Every domain value type of FR-R-007 shall implement `core::fmt::Display`, rendering exactly the wrapped value with no type name, no field name and no surrounding punctuation, so that `format!("unit {unit}")` composes without the caller stripping a wrapper. This is unconditional, not gated by any feature. `Debug` is unaffected and continues to show the wrapper.

**FR-R-153** — `FunctionCode` shall implement `core::fmt::Display`, unconditionally. A named code shall render as the English name FR-R-010 gives it, spelled exactly as FR-R-010 spells it — including `Read/Write Multiple Registers` and `Read FIFO Queue`. `Custom(u8)` shall render as `"Custom function "` followed by the decimal byte value.

**FR-R-154** — `ExceptionCode` shall implement `core::fmt::Display`, unconditionally. A named code shall render as the English name FR-R-082 gives it, spelled exactly as FR-R-082 spells it. `Other(u8)` shall render as `"Other exception "` followed by the decimal byte value.
