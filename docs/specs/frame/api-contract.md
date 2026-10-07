# Frame — API Contract

The stable public surface owned by the frame area: the set of Modbus function codes the crate names, the exported frame/PDU types and their signatures, and the error variants encoding and decoding can produce.

The set of *named* function codes is a **deliberate contract, not an open list** — adding a name is a normative change (gate 1). Codes outside it are not rejected; they are carried as `Custom(u8)` with an opaque body (FR-R-011, FR-R-012).

Shared by [`./`](.) (core, `FR-R-*`), [`../frame-data-access/`](../frame-data-access/) (`FR-DA-R-*`), and [`../frame-adu/`](../frame-adu/) (`FR-ADU-R-*`) — the frame area's public surface is one contract regardless of which sub-area's requirements a change touches.

---

## Function codes

All nineteen public function codes of the Modbus Application Protocol specification are named (FR-R-010):

| Code | Function | Notes | `Display` | Req |
|---|---|---|---|---|
| 1 / `0x01` | Read Coils | | `Read Coils` | FR-R-010, FR-R-153 |
| 2 / `0x02` | Read Discrete Inputs | | `Read Discrete Inputs` | FR-R-010, FR-R-153 |
| 3 / `0x03` | Read Holding Registers | | `Read Holding Registers` | FR-R-010, FR-R-153 |
| 4 / `0x04` | Read Input Registers | | `Read Input Registers` | FR-R-010, FR-R-153 |
| 5 / `0x05` | Write Single Coil | | `Write Single Coil` | FR-R-010, FR-R-153 |
| 6 / `0x06` | Write Single Register | | `Write Single Register` | FR-R-010, FR-R-153 |
| 7 / `0x07` | Read Exception Status | serial line only | `Read Exception Status` | FR-R-010, FR-R-153 |
| 8 / `0x08` | Diagnostics | serial line only; sub-functions in `## Diagnostics sub-functions (function code 8)` | `Diagnostics` | FR-R-010, FR-R-153 |
| 11 / `0x0B` | Get Comm Event Counter | serial line only | `Get Comm Event Counter` | FR-R-010, FR-R-153 |
| 12 / `0x0C` | Get Comm Event Log | serial line only | `Get Comm Event Log` | FR-R-010, FR-R-153 |
| 15 / `0x0F` | Write Multiple Coils | | `Write Multiple Coils` | FR-R-010, FR-R-153 |
| 16 / `0x10` | Write Multiple Registers | | `Write Multiple Registers` | FR-R-010, FR-R-153 |
| 17 / `0x11` | Report Server ID | serial line only | `Report Server ID` | FR-R-010, FR-R-153 |
| 20 / `0x14` | Read File Record | | `Read File Record` | FR-R-010, FR-R-153 |
| 21 / `0x15` | Write File Record | | `Write File Record` | FR-R-010, FR-R-153 |
| 22 / `0x16` | Mask Write Register | | `Mask Write Register` | FR-R-010, FR-R-153 |
| 23 / `0x17` | Read/Write Multiple Registers | | `Read/Write Multiple Registers` | FR-R-010, FR-R-153 |
| 24 / `0x18` | Read FIFO Queue | | `Read FIFO Queue` | FR-R-010, FR-R-153 |
| 43 / `0x2B` | Encapsulated Interface Transport | MEI types in `## MEI types (function code 43)` | `Encapsulated Interface Transport` | FR-R-010, FR-R-153 |
| *any other 1–127* | `Custom(u8)` | opaque body (FR-R-012) | `Custom function <n>` (FR-R-153) | FR-R-011, FR-R-012, FR-R-153 |

Code 0 is invalid; 128–255 are exception-response space and never denote a request (FR-R-014, FR-R-015).

**"Serial line only"** states where the specification *defines* the code to be used. The frame layer encodes and decodes it over any framing — restricting it by transport is the client's or server's judgment, not this area's.

## Diagnostics sub-functions (function code 8)

Named sub-functions (FR-R-062): 0 Return Query Data, 1 Restart Communications Option, 2 Return Diagnostic Register, 3 Change ASCII Input Delimiter, 4 Force Listen Only Mode, 10 Clear Counters and Diagnostic Register, 11 Return Bus Message Count, 12 Return Bus Communication Error Count, 13 Return Bus Exception Error Count, 14 Return Server Message Count, 15 Return Server No Response Count, 16 Return Server NAK Count, 17 Return Server Busy Count, 18 Return Bus Character Overrun Count, 20 Clear Overrun Counter and Flag.

Every other 16-bit sub-function code, including the reserved range 5–9, is carried as a general value holding the raw code (FR-R-063).

## MEI types (function code 43)

Named MEI types (FR-R-071): 13 CANopen General Reference, 14 Read Device Identification. Every other MEI type is carried as a general value holding the raw byte and an opaque body (FR-R-071).

## Exception codes

Named exception codes (FR-R-082), with the `Display` rendering FR-R-154 gives each:

| Code | Exception | `Display` | Req |
|---|---|---|---|
| 1 | Illegal Function | `Illegal Function` | FR-R-082, FR-R-154 |
| 2 | Illegal Data Address | `Illegal Data Address` | FR-R-082, FR-R-154 |
| 3 | Illegal Data Value | `Illegal Data Value` | FR-R-082, FR-R-154 |
| 4 | Server Device Failure | `Server Device Failure` | FR-R-082, FR-R-154 |
| 5 | Acknowledge | `Acknowledge` | FR-R-082, FR-R-154 |
| 6 | Server Device Busy | `Server Device Busy` | FR-R-082, FR-R-154 |
| 8 | Memory Parity Error | `Memory Parity Error` | FR-R-082, FR-R-154 |
| 10 | Gateway Path Unavailable | `Gateway Path Unavailable` | FR-R-082, FR-R-154 |
| 11 | Gateway Target Device Failed To Respond | `Gateway Target Device Failed To Respond` | FR-R-082, FR-R-154 |
| *any other byte, including 0* | `Other(u8)` | `Other exception <n>` | FR-R-083, FR-R-154 |

Every other byte, including 0, is carried as a general exception value holding the raw code (FR-R-083).

## Framings

| Framing | Wrapping | Integrity | Req |
|---|---|---|---|
| RTU | address + PDU + CRC | CRC-16, poly `0xA001`, init `0xFFFF`, low byte first | FR-R-090, FR-R-092, FR-R-093, FR-R-094 |
| RTU over stream | address + PDU + CRC, identical to RTU (FR-R-145) | the same CRC-16 | FR-R-145 |
| ASCII | `:` + hex(address + PDU + LRC) + CRLF | LRC, two's complement of the 8-bit sum | FR-R-110, FR-R-111, FR-R-114 |
| TCP | MBAP header + PDU | none (TCP provides it) | FR-R-100, FR-R-101 |

All three carry every function code in both directions (FR-R-118 states this explicitly for ASCII).

`crc16` and `lrc` (`## Exported types`) are exported standalone, so a caller can validate a captured frame's integrity without decoding it (FR-ADU-R-001, FR-ADU-R-002).

## Exported types

Everything below is exported from the crate root. All types derive `Debug`, `Clone`, and `PartialEq`; the field-free ones are `Copy` and `Eq` as well. The crate is `no_std` + `alloc` (NF-R-001), so `Vec` is `alloc::vec::Vec`. `Error` is the exception under the `tls` feature: `TlsHandshake` carries a `source: rustls::Error`, which has no `Eq`, so `Error`'s `Eq` derive is conditional on `tls` being off (TR-R-067).

| Item | Kind | Purpose | Req |
|---|---|---|---|
| `MAX_PDU_LEN: usize` | const | 253, the bound of FR-R-002 | FR-R-002 |
| `RequestPdu` | enum | one variant per named function code, plus `Custom { code, data }` | FR-R-005, FR-R-010, FR-R-011 |
| `ResponsePdu` | enum | the response direction, plus `Exception(ExceptionResponse)` | FR-R-005, FR-R-081 |
| `FunctionCode` | enum | the codes of `## Function codes`, plus `Custom(u8)` | FR-R-010, FR-R-011 |
| `ExceptionCode` | enum | the codes of `## Exception codes`, plus `Other(u8)` | FR-R-082, FR-R-083 |
| `ExceptionResponse` | struct | `{ function: FunctionCode, exception: ExceptionCode }` | FR-R-080, FR-R-081 |
| `DiagnosticSubFunction` | enum | the sub-functions of `## Diagnostics sub-functions (function code 8)`, plus `Other(u16)` | FR-R-062, FR-R-063 |
| `MeiRequest` / `MeiResponse` | enum | the MEI types of `## MEI types (function code 43)`, plus `Other { mei_type, data }` | FR-R-070, FR-R-071 |
| `ReadDeviceIdCode` | enum | `Basic`, `Regular`, `Extended`, `Individual` | FR-R-074 |
| `DeviceIdObject` | struct | `{ id: u8, value: Vec<u8> }` | FR-R-075 |
| `FileRecordRead` | struct | `{ file_number: FileNumber, record_number: RecordNumber, record_length: RecordLength }` | FR-R-050 |
| `FileRecordReadResponse` | struct | `{ values: Vec<RegisterValue> }` | FR-R-052 |
| `FileRecordWrite` | struct | `{ file_number: FileNumber, record_number: RecordNumber, values: Vec<RegisterValue> }` | FR-R-053 |
| `Framing` | trait | the ADU abstraction of FR-R-120 | FR-R-120 |
| `Rtu`, `RtuOverTcp`, `Ascii`, `Tcp` | struct | the four framings of `## Framings`, each a `Framing` impl | FR-R-090, FR-R-145, FR-R-110, FR-R-100 |
| `Direction` | enum | `Request`, `Response` — which direction a boundary derivation is asked about (FR-R-146) | FR-R-146 |
| `Extent` | enum | `NeedMore`, `Complete(usize)` — the result of a content-derived boundary derivation | FR-R-146 |
| `MbapHeader` | struct | `{ transaction_id: TransactionId, unit_id: UnitId }` | FR-R-101 |
| `Error`, `Result<T>` | enum, alias | `## Error variants`; `Result<T> = core::result::Result<T, Error>` | NF-R-012 |
| `mask_write_result` | fn | `(current: RegisterValue, and_mask: Mask, or_mask: Mask) -> RegisterValue` (FR-R-045) | FR-R-036 |
| `crc16` | fn | `(bytes: &[u8]) -> u16`, the RTU/RTU-over-stream CRC of `## Framings`, computable independently of decoding (FR-ADU-R-001) | FR-ADU-R-001 |
| `lrc` | fn | `(bytes: &[u8]) -> u8`, the ASCII checksum of `## Framings`, computable independently of decoding (FR-ADU-R-002) | FR-ADU-R-002 |

### Domain value types (FR-R-007)

Each is a transparent tuple struct with a public field, deriving `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, plus `From` in both directions with the integer it wraps. There is no fallible constructor: every value the wire can carry is constructible, and which of them is *sensible* stays the caller's judgement (FR-R-096 leaves addresses 248–255 to the caller) (FR-R-155).

| Type | Wraps | Carries | Req |
|---|---|---|---|
| `UnitId` | `u8` | the RTU/ASCII server address (FR-R-096, FR-R-117), the MBAP unit id (FR-R-101) | FR-R-007, FR-R-155 |
| `TransactionId` | `u16` | the MBAP transaction identifier (FR-R-101) | FR-R-007, FR-R-155 |
| `Address` | `u16` | every starting or single data address (`frame-data-access/requirements.md` `## Bit and register data access`) | FR-R-007, FR-R-155 |
| `Quantity` | `u16` | every count of coils, inputs, or registers (`frame-data-access/requirements.md` `## Bit and register data access`) | FR-R-007, FR-R-155 |
| `RegisterValue` | `u16` | register contents, FIFO contents, file record contents (FR-R-004) | FR-R-007, FR-R-155 |
| `Mask` | `u16` | the AND and OR masks of Mask Write Register (FR-R-044) | FR-R-007, FR-R-155 |
| `FileNumber`, `RecordNumber`, `RecordLength` | `u16` | the file record fields of `frame-data-access/requirements.md` `## File record access` | FR-R-007, FR-R-155 |
| `ExceptionStatus` | `u8` | the output status byte of Read Exception Status (FR-R-060) | FR-R-007, FR-R-155 |

Coil and discrete-input values stay `bool`: they are already unmixable with a 16-bit quantity. Diagnostic sub-function payload words, comm-event counters, and MEI object bytes stay raw integers — they are opaque data whose meaning the sub-function or MEI type decides, so a domain name would claim more than is true (FR-R-007).

Coding is symmetric and direction-explicit (FR-R-005): a PDU is not self-describing, so the caller states which direction it holds.

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

    // Required: the appending primitive (FR-R-140, FR-R-156).
    fn encode_request_into(header: &Self::Header, pdu: &RequestPdu, out: &mut Vec<u8>) -> Result<()>;
    fn encode_response_into(header: &Self::Header, pdu: &ResponsePdu, out: &mut Vec<u8>) -> Result<()>;

    // Provided: defined in terms of the above, so the two cannot disagree (FR-R-156).
    fn encode_request(header: &Self::Header, pdu: &RequestPdu) -> Result<Vec<u8>> { /* ... */ }
    fn encode_response(header: &Self::Header, pdu: &ResponsePdu) -> Result<Vec<u8>> { /* ... */ }
}
```

`encode_into` appends; it never clears what the buffer already holds (FR-R-140), and a failure truncates it back to the length it had on entry (FR-R-142), so a caller that reuses one buffer across frames never transmits a fragment of an abandoned one. The allocating `encode` is a wrapper over it (FR-R-156) rather than a second implementation, which is what keeps the two from describing different bytes.

`encode_request_into` reserves `MAX_ADU_LEN` before writing (FR-R-141), so every encode beneath it writes into capacity that already exists. The reservation is by framing maximum rather than by exact length because a PDU's length is settled only as its body is built; over-reserving by a few hundred bytes once is what buys an allocation-free steady state.

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

`is_self_locating` reports whether a failure is recoverable: `Silence` and `Delimited` locate the next boundary from the wire itself and are therefore self-locating, while `Prefixed` and `ContentLength` learn the next boundary only from the frame that just failed and are not (FR-R-144). The client and the server consult it to decide whether an undecodable frame costs one frame or the whole link (CL-R-098, SV-R-050).

`boundary` states where an ADU ends (FR-R-122) without performing any I/O, so the rule stays testable on byte vectors and available on `no_std`. `Tcp` is `Prefixed { prefix: 6, .. }` with `total` validating the MBAP length per FR-R-105 before returning `6 + length`; `Ascii` is `Delimited { start: b':', end: b"\r\n" }`; `Rtu` is `Silence`, whose duration is a serial-port property and therefore belongs to the transport area (TR-R-011), not here. `RtuOverTcp` is `ContentLength { min: 4, .. }` — an address, a one-byte PDU and a CRC are the least an ADU can be — whose `extent` implements FR-R-146 and FR-R-147.

`function` reports the code a PDU carries (FR-R-016) without re-encoding it; for `ResponsePdu::Exception` it is the code the exception is *to*, not the code with the high bit set, since that is the function the caller asked about.

`Framing::Header` is `UnitId` for `Rtu`, `RtuOverTcp` and `Ascii` (the server address, FR-R-096, FR-R-117) and `MbapHeader` for `Tcp` (FR-R-101). `MAX_ADU_LEN` is 256, 256, 513, and 260 respectively (FR-R-091, FR-R-113, FR-R-104).

`FunctionCode`, `ExceptionCode`, and `DiagnosticSubFunction` each expose `decode` and `encode`; the ones whose general variant can hold a named code (FR-R-013, FR-DA-R-005, FR-R-084) return `Result` on encode, the others do not.

## Error variants

One enum, `Error`, with a variant per failure mode — never a formatted string a caller has to match on by substring (NF-R-012). Adding a variant is a normative change.

| Variant | Fields | Req |
|---|---|---|
| `Truncated` | `expected: usize, supplied: usize` | FR-R-131 |
| `TrailingBytes` | `extra: usize` | FR-R-132 |
| `InvalidFunctionCode` | `u8` | FR-R-014, FR-R-015 |
| `ReservedCode` | `u8` | FR-R-013, FR-DA-R-005, FR-R-084 |
| `InvalidLength` | `expected: usize, actual: usize` | FR-R-084, FR-R-085, FR-R-106 |
| `OutOfRange` | `field: &'static str, value: u32, min: u32, max: u32` | FR-R-021, FR-R-027, FR-R-031, FR-R-038, FR-R-042, FR-R-051, FR-R-055, FR-R-105, FR-DA-R-006 |
| `Checksum` | `expected: u16, actual: u16` | FR-R-095, FR-R-115 |
| `Framing` | `element: &'static str` | FR-R-110, FR-R-116 |
| `InvalidCharacter` | `u8` | FR-ADU-R-004 |
| `ProtocolIdentifier` | `u16` | FR-R-102 |
| `AduTooLarge` | `len: usize, max: usize` | FR-R-091, FR-R-104, FR-R-113, FR-R-149 |
| `IndeterminateLength` | `function: u8` | FR-R-148 |
| `ReferenceType` | `u8` | FR-R-054 |
| `IllegalValue` | `field: &'static str, value: u16` | FR-R-022, FR-DA-R-002, FR-DA-R-004, FR-R-074, FR-R-077 |
| `ByteCountMismatch` | `expected: usize, actual: usize` | FR-R-033, FR-R-043, FR-R-056, FR-R-057, FR-R-058, FR-R-076 |
| `PduTooLarge` | `len: usize, max: usize` | FR-R-002, FR-R-006 |
| `Malformed` | — (the residual: input that fits no other variant) | — |

`Error` implements `core::error::Error` via `thiserror`, so it is usable in `no_std` builds and composes with `std::error::Error` where `std` is present.

## Serde support (feature-gated)

Behind the crate's `serde` feature (NF-R-025), the ten domain value types of `### Domain value types (FR-R-007)` — `UnitId`, `TransactionId`, `Address`, `Quantity`, `RegisterValue`, `Mask`, `FileNumber`, `RecordNumber`, `RecordLength`, `ExceptionStatus` — implement `serde::Serialize` and `serde::Deserialize` as `#[serde(transparent)]` (FR-R-151): each serializes and deserializes as its bare wrapped integer, with no wrapping structure of its own. No other frame type (`RequestPdu`, `ResponsePdu`, `MbapHeader`, `ExceptionCode`, `FunctionCode`, or any other frame type) implements either trait.

## Display

Unconditional, not feature-gated:

- Every domain value type of `### Domain value types (FR-R-007)` implements `core::fmt::Display`, rendering exactly its wrapped value (FR-R-152).
- `FunctionCode` implements `core::fmt::Display` per the table in `## Function codes` (FR-R-153).
- `ExceptionCode` implements `core::fmt::Display` per the table in `## Exception codes` (FR-R-154).
