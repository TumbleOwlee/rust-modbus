# Frame — API Contract

The stable public surface owned by the frame area: the set of Modbus function
codes the crate names, the exported frame/PDU types and their signatures, and the
error variants encoding and decoding can produce.

The set of *named* function codes is a **deliberate contract, not an open list** —
adding a name is a normative change (gate 1). Codes outside it are not rejected;
they are carried as `Custom(u8)` with an opaque body (FR-R-011, FR-R-012).

Shared by [`./`](.) (core, `FR-R-*`), [`../frame-data-access/`](../frame-data-access/)
(`FR-DA-R-*`), and [`../frame-adu/`](../frame-adu/) (`FR-ADU-R-*`) — the frame
area's public surface is one contract regardless of which sub-area's
requirements a change touches.

---

## 1. Function codes

All nineteen public function codes of the Modbus Application Protocol
specification are named (FR-R-010):

| Code | Function | Notes | `Display` |
|---|---|---|---|
| 1 / `0x01` | Read Coils | | `Read Coils` |
| 2 / `0x02` | Read Discrete Inputs | | `Read Discrete Inputs` |
| 3 / `0x03` | Read Holding Registers | | `Read Holding Registers` |
| 4 / `0x04` | Read Input Registers | | `Read Input Registers` |
| 5 / `0x05` | Write Single Coil | | `Write Single Coil` |
| 6 / `0x06` | Write Single Register | | `Write Single Register` |
| 7 / `0x07` | Read Exception Status | serial line only | `Read Exception Status` |
| 8 / `0x08` | Diagnostics | serial line only; sub-functions in §2 | `Diagnostics` |
| 11 / `0x0B` | Get Comm Event Counter | serial line only | `Get Comm Event Counter` |
| 12 / `0x0C` | Get Comm Event Log | serial line only | `Get Comm Event Log` |
| 15 / `0x0F` | Write Multiple Coils | | `Write Multiple Coils` |
| 16 / `0x10` | Write Multiple Registers | | `Write Multiple Registers` |
| 17 / `0x11` | Report Server ID | serial line only | `Report Server ID` |
| 20 / `0x14` | Read File Record | | `Read File Record` |
| 21 / `0x15` | Write File Record | | `Write File Record` |
| 22 / `0x16` | Mask Write Register | | `Mask Write Register` |
| 23 / `0x17` | Read/Write Multiple Registers | | `Read/Write Multiple Registers` |
| 24 / `0x18` | Read FIFO Queue | | `Read FIFO Queue` |
| 43 / `0x2B` | Encapsulated Interface Transport | MEI types in §3 | `Encapsulated Interface Transport` |
| *any other 1–127* | `Custom(u8)` | opaque body (FR-R-012) | `Custom function <n>` (FR-R-153) |

Code 0 is invalid; 128–255 are exception-response space and never denote a
request (FR-R-014, FR-R-015).

**"Serial line only"** states where the specification *defines* the code to be
used. The frame layer encodes and decodes it over any framing — restricting it by
transport is the client's or server's judgment, not this area's.

## 2. Diagnostics sub-functions (function code 8)

Named sub-functions (FR-R-062): 0 Return Query Data, 1 Restart Communications
Option, 2 Return Diagnostic Register, 3 Change ASCII Input Delimiter, 4 Force
Listen Only Mode, 10 Clear Counters and Diagnostic Register, 11 Return Bus
Message Count, 12 Return Bus Communication Error Count, 13 Return Bus Exception
Error Count, 14 Return Server Message Count, 15 Return Server No Response Count,
16 Return Server NAK Count, 17 Return Server Busy Count, 18 Return Bus Character
Overrun Count, 20 Clear Overrun Counter and Flag.

Every other 16-bit sub-function code, including the reserved range 5–9, is
carried as a general value holding the raw code (FR-R-063).

## 3. MEI types (function code 43)

Named MEI types (FR-R-071): 13 CANopen General Reference, 14 Read Device
Identification. Every other MEI type is carried as a general value holding the
raw byte and an opaque body.

## 4. Exception codes

Named exception codes (FR-R-082), with the `Display` rendering FR-R-154 gives
each:

| Code | Exception | `Display` |
|---|---|---|
| 1 | Illegal Function | `Illegal Function` |
| 2 | Illegal Data Address | `Illegal Data Address` |
| 3 | Illegal Data Value | `Illegal Data Value` |
| 4 | Server Device Failure | `Server Device Failure` |
| 5 | Acknowledge | `Acknowledge` |
| 6 | Server Device Busy | `Server Device Busy` |
| 8 | Memory Parity Error | `Memory Parity Error` |
| 10 | Gateway Path Unavailable | `Gateway Path Unavailable` |
| 11 | Gateway Target Device Failed To Respond | `Gateway Target Device Failed To Respond` |
| *any other byte, including 0* | `Other(u8)` | `Other exception <n>` |

Every other byte, including 0, is carried as a general exception value holding
the raw code (FR-R-083).

## 5. Framings

| Framing | Wrapping | Integrity |
|---|---|---|
| RTU | address + PDU + CRC | CRC-16, poly `0xA001`, init `0xFFFF`, low byte first |
| RTU over stream | address + PDU + CRC, identical to RTU (FR-R-145) | the same CRC-16 |
| ASCII | `:` + hex(address + PDU + LRC) + CRLF | LRC, two's complement of the 8-bit sum |
| TCP | MBAP header + PDU | none (TCP provides it) |

All three carry every function code in both directions (FR-R-118 states this
explicitly for ASCII).

## 6. Exported types

Everything below is exported from the crate root. All types derive `Debug`,
`Clone`, and `PartialEq`; the field-free ones are `Copy` and `Eq` as well. The
crate is `no_std` + `alloc` (NF-R-001), so `Vec` is `alloc::vec::Vec`.

| Item | Kind | Purpose |
|---|---|---|
| `MAX_PDU_LEN: usize` | const | 253, the bound of FR-R-002 |
| `RequestPdu` | enum | one variant per named function code, plus `Custom { code, data }` |
| `ResponsePdu` | enum | the response direction, plus `Exception(ExceptionResponse)` |
| `FunctionCode` | enum | the codes of §1, plus `Custom(u8)` |
| `ExceptionCode` | enum | the codes of §4, plus `Other(u8)` |
| `ExceptionResponse` | struct | `{ function: FunctionCode, exception: ExceptionCode }` |
| `DiagnosticSubFunction` | enum | the sub-functions of §2, plus `Other(u16)` |
| `MeiRequest` / `MeiResponse` | enum | the MEI types of §3, plus `Other { mei_type, data }` |
| `ReadDeviceIdCode` | enum | `Basic`, `Regular`, `Extended`, `Individual` |
| `DeviceIdObject` | struct | `{ id: u8, value: Vec<u8> }` |
| `FileRecordRead` | struct | `{ file_number: FileNumber, record_number: RecordNumber, record_length: RecordLength }` |
| `FileRecordReadResponse` | struct | `{ values: Vec<RegisterValue> }` |
| `FileRecordWrite` | struct | `{ file_number: FileNumber, record_number: RecordNumber, values: Vec<RegisterValue> }` |
| `Framing` | trait | the ADU abstraction of FR-R-120 |
| `Rtu`, `RtuOverTcp`, `Ascii`, `Tcp` | struct | the four framings of §5, each a `Framing` impl |
| `Direction` | enum | `Request`, `Response` — which direction a boundary derivation is asked about (FR-R-146) |
| `Extent` | enum | `NeedMore`, `Complete(usize)` — the result of a content-derived boundary derivation |
| `MbapHeader` | struct | `{ transaction_id: TransactionId, unit_id: UnitId }` |
| `Error`, `Result<T>` | enum, alias | §7; `Result<T> = core::result::Result<T, Error>` |
| `mask_write_result` | fn | `(current: RegisterValue, and_mask: Mask, or_mask: Mask) -> RegisterValue` (FR-R-045) |

### Domain value types (FR-R-007)

Each is a transparent tuple struct with a public field, deriving `Debug`,
`Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, plus `From` in
both directions with the integer it wraps. There is no fallible constructor:
every value the wire can carry is constructible, and which of them is *sensible*
stays the caller's judgement (FR-R-096 leaves addresses 248–255 to the caller).

| Type | Wraps | Carries |
|---|---|---|
| `UnitId` | `u8` | the RTU/ASCII server address (FR-R-096, FR-R-117), the MBAP unit id (FR-R-101) |
| `TransactionId` | `u16` | the MBAP transaction identifier (FR-R-101) |
| `Address` | `u16` | every starting or single data address (§3, §6) |
| `Quantity` | `u16` | every count of coils, inputs, or registers (§3) |
| `RegisterValue` | `u16` | register contents, FIFO contents, file record contents (FR-R-004) |
| `Mask` | `u16` | the AND and OR masks of Mask Write Register (FR-R-044) |
| `FileNumber`, `RecordNumber`, `RecordLength` | `u16` | the file record fields of §4 |
| `ExceptionStatus` | `u8` | the output status byte of Read Exception Status (FR-R-060) |

Coil and discrete-input values stay `bool`: they are already unmixable with a
16-bit quantity. Diagnostic sub-function payload words, comm-event counters, and
MEI object bytes stay raw integers — they are opaque data whose meaning the
sub-function or MEI type decides, so a domain name would claim more than is true.

Coding is symmetric and direction-explicit (FR-R-005): a PDU is not
self-describing, so the caller states which direction it holds.

```rust
impl RequestPdu {
    pub fn decode(pdu: &[u8]) -> Result<Self>;
    pub fn encode(&self) -> Result<Vec<u8>>;
    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<()>;
    pub fn function(&self) -> FunctionCode;
}
impl ResponsePdu {
    pub fn decode(pdu: &[u8]) -> Result<Self>;
    pub fn encode(&self) -> Result<Vec<u8>>;
    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<()>;
    pub fn function(&self) -> FunctionCode;
}

pub trait Framing {
    type Header: Clone + PartialEq + Debug;
    const MAX_ADU_LEN: usize;
    fn decode_request(bytes: &[u8]) -> Result<(Self::Header, RequestPdu)>;
    fn decode_response(bytes: &[u8]) -> Result<(Self::Header, ResponsePdu)>;
    fn boundary() -> AduBoundary;

    // Required: the appending primitive (FR-R-140).
    fn encode_request_into(header: &Self::Header, pdu: &RequestPdu, out: &mut Vec<u8>) -> Result<()>;
    fn encode_response_into(header: &Self::Header, pdu: &ResponsePdu, out: &mut Vec<u8>) -> Result<()>;

    // Provided: defined in terms of the above, so the two cannot disagree.
    fn encode_request(header: &Self::Header, pdu: &RequestPdu) -> Result<Vec<u8>> { /* ... */ }
    fn encode_response(header: &Self::Header, pdu: &ResponsePdu) -> Result<Vec<u8>> { /* ... */ }
}
```

`encode_into` appends; it never clears what the buffer already holds, and a
failure truncates it back to the length it had on entry (FR-R-142), so a caller
that reuses one buffer across frames never transmits a fragment of an abandoned
one. The allocating `encode` is a wrapper over it (FR-R-140) rather than a second
implementation, which is what keeps the two from describing different bytes.

`encode_request_into` reserves `MAX_ADU_LEN` before writing (FR-R-141), so every
encode beneath it writes into capacity that already exists. The reservation is by
framing maximum rather than by exact length because a PDU's length is settled only
as its body is built; over-reserving by a few hundred bytes once is what buys an
allocation-free steady state.

```rust
pub enum AduBoundary {
    /// Read `prefix` bytes, then `total` yields the whole ADU's length.
    Prefixed { prefix: usize, total: fn(&[u8]) -> Result<usize> },
    /// The ADU runs from `start` to the first `end` following it.
    Delimited { start: u8, end: &'static [u8] },
    /// The ADU ends when the line goes quiet.
    Silence,
    /// The ADU's own content gives its length. `extent` is called once at
    /// least `min` bytes are in hand, and again after every further read.
    ContentLength { min: usize, extent: fn(Direction, &[u8]) -> Result<Extent> },
}

/// Which direction a boundary derivation is being asked about (FR-R-005).
pub enum Direction { Request, Response }

/// What a content-derived boundary makes of the bytes so far.
pub enum Extent {
    /// Not enough bytes yet to say where the ADU ends.
    NeedMore,
    /// The ADU is exactly this many bytes long, that count including any bytes
    /// still to be read.
    Complete(usize),
}
```

```rust
impl AduBoundary {
    /// Whether the next frame boundary is findable from the wire alone (FR-R-144).
    pub fn is_self_locating(&self) -> bool;
}
```

`is_self_locating` reports whether a failure is recoverable: `Silence` and `Delimited`
locate the next boundary from the wire itself and are therefore self-locating, while
`Prefixed` and `ContentLength` learn the next boundary only from the frame that just
failed and are not. The client and the server consult it to decide whether an
undecodable frame costs one frame or the whole link (CL-R-023, SV-R-050).

`boundary` states where an ADU ends (FR-R-122) without performing any I/O, so
the rule stays testable on byte vectors and available on `no_std`. `Tcp` is
`Prefixed { prefix: 6, .. }` with `total` validating the MBAP length per
FR-R-105 before returning `6 + length`; `Ascii` is `Delimited { start: b':',
end: b"\r\n" }`; `Rtu` is `Silence`, whose duration is a serial-port property and
therefore belongs to the transport area (TR-R-011), not here. `RtuOverTcp` is
`ContentLength { min: 4, .. }` — an address, a one-byte PDU and a CRC are the
least an ADU can be — whose `extent` implements FR-R-146 and FR-R-147.

`function` reports the code a PDU carries (FR-R-016) without re-encoding it; for
`ResponsePdu::Exception` it is the code the exception is *to*, not the code with
the high bit set, since that is the function the caller asked about.

`Framing::Header` is `UnitId` for `Rtu`, `RtuOverTcp` and `Ascii` (the server
address, FR-R-096, FR-R-117) and `MbapHeader` for `Tcp` (FR-R-101). `MAX_ADU_LEN`
is 256, 256, 513, and 260 respectively (FR-R-091, FR-R-113, FR-R-104).

`FunctionCode`, `ExceptionCode`, and `DiagnosticSubFunction` each expose
`decode` and `encode`; the ones whose general variant can hold a named code
(FR-R-013, FR-R-063, FR-R-083) return `Result` on encode, the others do not.

## 7. Error variants

One enum, `Error`, with a variant per failure mode — never a formatted string a
caller has to match on by substring. Adding a variant is a normative change.

| Variant | Fields | Requirements |
|---|---|---|
| `Truncated` | `expected: usize, supplied: usize` | FR-R-131 |
| `TrailingBytes` | `extra: usize` | FR-R-132 |
| `InvalidFunctionCode` | `u8` | FR-R-014, FR-R-015 |
| `ReservedCode` | `u8` | FR-R-013, FR-R-063, FR-R-083 |
| `InvalidLength` | `expected: usize, actual: usize` | FR-R-084, FR-R-085, FR-R-106 |
| `OutOfRange` | `field: &'static str, value: u32, min: u32, max: u32` | FR-R-021, FR-R-027, FR-R-031, FR-R-038, FR-R-042, FR-R-051, FR-R-055, FR-R-105 |
| `Checksum` | `expected: u16, actual: u16` | FR-R-095, FR-R-115 |
| `Framing` | `element: &'static str` | FR-R-110, FR-R-116 |
| `InvalidCharacter` | `u8` | FR-R-112 |
| `ProtocolIdentifier` | `u16` | FR-R-102 |
| `AduTooLarge` | `len: usize, max: usize` | FR-R-091, FR-R-104, FR-R-113, FR-R-149 |
| `IndeterminateLength` | `function: u8` | FR-R-148 |
| `ReferenceType` | `u8` | FR-R-054 |
| `IllegalValue` | `field: &'static str, value: u16` | FR-R-022, FR-R-061, FR-R-065, FR-R-074, FR-R-077 |
| `ByteCountMismatch` | `expected: usize, actual: usize` | FR-R-033, FR-R-043, FR-R-056, FR-R-057, FR-R-058, FR-R-076 |
| `PduTooLarge` | `len: usize, max: usize` | FR-R-002, FR-R-006 |
| `Malformed` | — | the residual: input that fits no other variant |

`Error` implements `core::error::Error` via `thiserror`, so it is usable in
`no_std` builds and composes with `std::error::Error` where `std` is present.

## 8. Serde support (feature-gated)

Behind the crate's `serde` feature (NF-R-025), the ten domain value types of
§"Domain value types (FR-R-007)" — `UnitId`, `TransactionId`, `Address`,
`Quantity`, `RegisterValue`, `Mask`, `FileNumber`, `RecordNumber`,
`RecordLength`, `ExceptionStatus` — implement `serde::Serialize` and
`serde::Deserialize` as `#[serde(transparent)]` (FR-R-151): each serializes and
deserializes as its bare wrapped integer, with no wrapping structure of its own.
No other frame type (`RequestPdu`, `ResponsePdu`, `MbapHeader`, `ExceptionCode`,
`FunctionCode`, or any other frame type) implements either trait.

## 9. Display

Unconditional, not feature-gated:

- Every domain value type of §"Domain value types (FR-R-007)" implements
  `core::fmt::Display`, rendering exactly its wrapped value (FR-R-152).
- `FunctionCode` implements `core::fmt::Display` per the table in §1 (FR-R-153).
- `ExceptionCode` implements `core::fmt::Display` per the table in §4
  (FR-R-154).
