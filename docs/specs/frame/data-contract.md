# Frame — Data Contract

Wire formats owned by the frame area: the PDU layout per function code, the RTU, ASCII, and TCP ADU wrappings, byte order, field widths, and valid ranges.

Expected byte sequences here are derived from the published Modbus specifications (*Modbus Application Protocol V1.1b3*, *Modbus over Serial Line V1.02*) — never from this implementation's own output. A data contract that documents what the encoder happens to do is worthless as a check on it.

All multi-byte numeric fields are big-endian (FR-R-003). "qty" abbreviates quantity. Widths are in bytes unless stated.

Shared by [`./`](.) (core, `FR-R-*`), [`../frame-data-access/`](../frame-data-access/) (`FR-DA-R-*`), and [`../frame-adu/`](../frame-adu/) (`FR-ADU-R-*`) — the frame area's wire formats are one contract regardless of which sub-area's requirements a change touches.

The layouts below are stated in wire widths. In the API each of these fields is a domain value type (FR-R-007, FR-R-155) transparently wrapping that width: a 2-byte address field is an `Address`, a qty an `Quantity`, a register a `RegisterValue`, a mask a `Mask`, the file record fields `FileNumber`, `RecordNumber`, `RecordLength`, the 1-byte server address a `UnitId`, and the MBAP transaction identifier a `TransactionId`. The wrapper changes no byte on the wire; it exists so two fields of equal width and unequal meaning cannot be swapped.

---

## PDU layouts

### Bit and register access

| Code | Request body | Response body | Req |
|---|---|---|---|
| 1, 2 | start addr (2), qty (2) | byte count (1), data (`(qty+7)/8`) | FR-R-020, FR-R-023 |
| 3, 4 | start addr (2), qty (2) | byte count (1), data (`2×qty`) | FR-R-020, FR-R-025 |
| 5 | output addr (2), value (2) = `0xFF00` \| `0x0000` | echo of request | FR-R-026, FR-R-029 |
| 6 | register addr (2), value (2) | echo of request | FR-R-028, FR-R-029 |
| 15 | start addr (2), qty (2), byte count (1), data (`(qty+7)/8`) | start addr (2), qty (2) | FR-R-030, FR-R-034 |
| 16 | start addr (2), qty (2), byte count (1), data (`2×qty`) | start addr (2), qty (2) | FR-R-032, FR-R-034 |
| 22 | reference addr (2), AND mask (2), OR mask (2) | echo of request | FR-R-035, FR-DA-R-001 |
| 23 | read addr (2), read qty (2), write addr (2), write qty (2), write byte count (1), write data (`2×write qty`) | byte count (1), data (`2×read qty`) | FR-R-037, FR-R-039 |
| 24 | FIFO pointer addr (2) | byte count (**2**), FIFO count (2), data (`2×FIFO count`) | FR-R-040, FR-R-041 |

Bit packing (FR-R-024): bit *n* of the range occupies bit `n mod 8` of data byte `n / 8`; the least significant bit of the first data byte is the first coil. Unused high bits of the final byte are zero.

Function code 24 is the only one whose byte count is two bytes wide, and it counts the FIFO count field as well as the data (`2×FIFO count + 2`) (FR-R-041).

### File record access

| Code | Request body | Response body | Req |
|---|---|---|---|
| 20 | byte count (1), then *N* sub-requests | data length (1), then *N* sub-responses | FR-R-050, FR-R-052 |
| 21 | data length (1), then *N* sub-requests | echo of request | FR-R-053, FR-DA-R-003 |

- **FC20 sub-request** (7 bytes, fixed): reference type (1) = 6, file number (2), record number (2), record length in registers (2) (FR-R-050, FR-R-055).
- **FC20 sub-response**: file response length (1) = record data bytes + 1, reference type (1) = 6, record data (`2×record length`) (FR-R-052, FR-R-055).
- **FC21 sub-request**: reference type (1) = 6, file number (2), record number (2), record length in registers (2), record data (`2×record length`) (FR-R-053, FR-R-055).

### Serial-line diagnostics

| Code | Request body | Response body | Req |
|---|---|---|---|
| 7 | *(empty)* | output data (1) | FR-R-060 |
| 8 | sub-function (2), data (`2×n`, n ≥ 0) | sub-function (2), data (`2×n`) | FR-R-061 |
| 11 | *(empty)* | status (2), event count (2) | FR-R-064 |
| 12 | *(empty)* | byte count (1) = events + 6, status (2), event count (2), message count (2), events (0–64) | FR-R-065 |
| 17 | *(empty)* | byte count (1), server id (device-specific), run indicator (1) = `0x00` \| `0xFF`, additional data — the interior is carried, not parsed (FR-R-067) | FR-R-066, FR-R-067 |

Status word (FR-R-068): `0xFFFF` while busy with a program function, `0x0000` otherwise; other values are carried, not rejected.

### Encapsulated Interface Transport (code 43)

Both directions open with MEI type (1) (FR-R-070).

| MEI | Request body | Response body | Req |
|---|---|---|---|
| 13 | opaque (all remaining bytes) | opaque (all remaining bytes) | FR-R-072 |
| 14 | read device id code (1) = 1–4, object id (1) | read device id code (1), conformity level (1), more follows (1) = `0x00` \| `0xFF`, next object id (1), object count (1), objects | FR-R-073, FR-R-075 |
| *other* | opaque (all remaining bytes) | opaque (all remaining bytes) | FR-R-071 |

- **Object** (MEI 14 response): object id (1), object length (1), object value (`object length`) (FR-R-075).

### Exception response

| Field | Width | Req |
|---|---|---|
| function code \| `0x80` | 1 | FR-R-080 |
| exception code | 1 | FR-R-080 |

Total PDU length is exactly 2 (FR-R-085).

---

## RTU ADU

| Field | Width | Req |
|---|---|---|
| address | 1 | FR-R-090 |
| PDU | 1–253 | FR-R-090, FR-R-002 |
| CRC | 2 | FR-R-090 |

CRC-16 parameters (FR-R-092, FR-R-093, FR-R-094):

| Parameter | Value | Req |
|---|---|---|
| Polynomial (reversed) | `0xA001` | FR-R-092 |
| Initial value | `0xFFFF` | FR-R-092 |
| Covered bytes | address + entire PDU, nothing else | FR-R-093 |
| Transmission order | low byte, then high byte | FR-R-094 |

---

## ASCII ADU

| Field | Characters | Req |
|---|---|---|
| start | 1 (`:`, `0x3A`) | FR-R-110 |
| address | 2 | FR-R-110 |
| PDU | 2–506 | FR-R-110, FR-R-111 |
| LRC | 2 | FR-R-110 |
| terminator | 2 (CR LF, `0x0D 0x0A`) | FR-R-110 |

Each byte becomes exactly two ASCII hexadecimal characters, most significant nibble first (FR-R-111). Encoding emits uppercase (FR-R-112); decoding accepts either case (FR-ADU-R-003).

LRC (FR-R-114): the two's complement of the 8-bit sum of the **decoded** address byte and every decoded PDU byte — `LRC = (0x100 - (sum & 0xFF)) & 0xFF`. It is never computed over the hexadecimal characters themselves.

---

## TCP ADU

| Field | Width | Req |
|---|---|---|
| transaction identifier | 2 | FR-R-101 |
| protocol identifier | 2 (always 0) | FR-R-101, FR-R-102 |
| length | 2 | FR-R-101, FR-R-103 |
| unit identifier | 1 | FR-R-101 |
| PDU | 1–253 | FR-R-100, FR-R-002 |

The length field counts the bytes that follow it: the unit identifier plus the PDU, i.e. `PDU length + 1` (FR-R-103), giving a valid range of 2–254.

---

## Ranges and limits

| Limit | Value | Req |
|---|---|---|
| Max PDU | 253 bytes | FR-R-002 |
| Max RTU ADU | 256 bytes | FR-R-091 |
| Max ASCII ADU | 513 characters (255 encoded bytes) | FR-R-113 |
| Max TCP ADU | 260 bytes | FR-R-104 |
| Read Coils, Read Discrete Inputs qty | 1–2000 | FR-R-021 |
| Read Holding, Read Input Registers qty | 1–125 | FR-R-022 |
| Write Multiple Coils qty | 1–1968 | FR-R-031 |
| Write Multiple Registers qty | 1–123 | FR-R-033 |
| FC23 read qty / write qty | 1–125 / 1–121 | FR-R-038 |
| FC24 FIFO count | 0–31 | FR-R-042 |
| FC20 request byte count | 7–245, exact multiple of 7 | FR-R-051, FR-DA-R-002 |
| FC20 response data length | 7–245 | FR-R-052 |
| FC21 request data length | 9–251 | FR-R-054 |
| File number / record number | 1–65535 / 0–9999 | FR-R-056 |
| Read device id code | 1–4 | FR-R-074 |
| Get Comm Event Log events | 0–64 bytes | FR-R-065 |
| MBAP length field | 2–254 | FR-R-103, FR-R-105 |
| Serial address | 0 broadcast, 1–247 individual, 248–255 carried | FR-R-096 |
| Address space per register table | 0–65535 | — |

---

## RTU-over-stream extents (FR-R-147)

PDU length yielded by the derivation, by function code and direction:

| Direction | Function code(s) | PDU length | Req |
|---|---|---|---|
| response, exception (MSB set) | any, including custom | 2 (FR-R-080, FR-R-086) | FR-R-147 |
| request | 1, 2, 3, 4, 5, 6 | 5 | FR-R-147 |
| request | 7, 11, 12, 17 | 1 | FR-R-147 |
| request | 22 | 7 | FR-R-147 |
| request | 24 | 3 | FR-R-147 |
| request | 15, 16 | `6 + byte count` (byte count = 6th PDU byte) | FR-R-147 |
| request | 20, 21 | `2 + byte count` (byte count = 2nd PDU byte) | FR-R-147 |
| request | 23 | `10 + write byte count` (write byte count = 10th PDU byte) | FR-R-147 |
| request | 43, MEI type 14 | 4 | FR-R-147 |
| response | 5, 6, 15, 16, 11 | 5 | FR-R-147 |
| response | 7 | 2 | FR-R-147 |
| response | 22 | 7 | FR-R-147 |
| response | 1, 2, 3, 4, 12, 17, 20, 21, 23 | `2 + byte count` (byte count = 2nd PDU byte) | FR-R-147 |
| response | 24 | `3 + byte count` (byte count = 2-byte field, FR-R-041) | FR-R-147 |
| response | 43, MEI type 14 | walk object list per FR-R-075, each object contributing `2 + object length` bytes | FR-R-147 |
