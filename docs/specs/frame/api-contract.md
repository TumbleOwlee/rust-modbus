# Frame — API Contract

The stable public surface owned by the frame area: the set of Modbus function
codes the crate names, the exported frame/PDU types and their signatures, and the
error variants encoding and decoding can produce.

The set of *named* function codes is a **deliberate contract, not an open list** —
adding a name is a normative change (gate 1). Codes outside it are not rejected;
they are carried as `Custom(u8)` with an opaque body (FR-R-011, FR-R-012).

---

## 1. Function codes

All nineteen public function codes of the Modbus Application Protocol
specification are named (FR-R-010):

| Code | Function | Notes |
|---|---|---|
| 1 / `0x01` | Read Coils | |
| 2 / `0x02` | Read Discrete Inputs | |
| 3 / `0x03` | Read Holding Registers | |
| 4 / `0x04` | Read Input Registers | |
| 5 / `0x05` | Write Single Coil | |
| 6 / `0x06` | Write Single Register | |
| 7 / `0x07` | Read Exception Status | serial line only |
| 8 / `0x08` | Diagnostics | serial line only; sub-functions in §2 |
| 11 / `0x0B` | Get Comm Event Counter | serial line only |
| 12 / `0x0C` | Get Comm Event Log | serial line only |
| 15 / `0x0F` | Write Multiple Coils | |
| 16 / `0x10` | Write Multiple Registers | |
| 17 / `0x11` | Report Server ID | serial line only |
| 20 / `0x14` | Read File Record | |
| 21 / `0x15` | Write File Record | |
| 22 / `0x16` | Mask Write Register | |
| 23 / `0x17` | Read/Write Multiple Registers | |
| 24 / `0x18` | Read FIFO Queue | |
| 43 / `0x2B` | Encapsulated Interface Transport | MEI types in §3 |
| *any other 1–127* | `Custom(u8)` | opaque body (FR-R-012) |

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

Named exception codes (FR-R-082): 1 Illegal Function, 2 Illegal Data Address,
3 Illegal Data Value, 4 Server Device Failure, 5 Acknowledge, 6 Server Device
Busy, 8 Memory Parity Error, 10 Gateway Path Unavailable, 11 Gateway Target
Device Failed To Respond.

Every other byte, including 0, is carried as a general exception value holding
the raw code (FR-R-083).

## 5. Framings

| Framing | Wrapping | Integrity |
|---|---|---|
| RTU | address + PDU + CRC | CRC-16, poly `0xA001`, init `0xFFFF`, low byte first |
| ASCII | `:` + hex(address + PDU + LRC) + CRLF | LRC, two's complement of the 8-bit sum |
| TCP | MBAP header + PDU | none (TCP provides it) |

All three carry every function code in both directions (FR-R-118 states this
explicitly for ASCII).

## 6. Exported types

*(TBD — the public types this area exports and their signatures. Settled at
gate 2 with the code in front of us; the behavior they must exhibit is already
fixed by `requirements.md`.)*

## 7. Error variants

*(TBD — the Rust spelling of the failure modes. The modes themselves are
normative and named by FR-R-006, FR-R-013, FR-R-014, FR-R-015, FR-R-021,
FR-R-022, FR-R-027, FR-R-031, FR-R-033, FR-R-038, FR-R-042, FR-R-043, FR-R-051,
FR-R-054, FR-R-055, FR-R-056, FR-R-057, FR-R-058, FR-R-061, FR-R-063,
FR-R-065, FR-R-074, FR-R-076, FR-R-077, FR-R-084,
FR-R-085, FR-R-095, FR-R-102, FR-R-105, FR-R-106, FR-R-112, FR-R-115, FR-R-116,
FR-R-131, FR-R-132. Adding a variant beyond these is a normative change.)*
