# Frame — Data Contract

Wire formats owned by the frame area: the PDU layout per function code, the RTU,
ASCII, and TCP ADU wrappings, byte order, field widths, and valid ranges.

Expected byte sequences here are derived from the published Modbus
specifications (*Modbus Application Protocol V1.1b3*, *Modbus over Serial Line
V1.02*) — never from this implementation's own output. A data contract that
documents what the encoder happens to do is worthless as a check on it.

All multi-byte numeric fields are big-endian (FR-R-003). "qty" abbreviates
quantity. Widths are in bytes unless stated.

---

## 1. PDU layouts

### 1.1 Bit and register access

| Code | Request body | Response body |
|---|---|---|
| 1, 2 | start addr (2), qty (2) | byte count (1), data (`(qty+7)/8`) |
| 3, 4 | start addr (2), qty (2) | byte count (1), data (`2×qty`) |
| 5 | output addr (2), value (2) = `0xFF00` \| `0x0000` | echo of request |
| 6 | register addr (2), value (2) | echo of request |
| 15 | start addr (2), qty (2), byte count (1), data (`(qty+7)/8`) | start addr (2), qty (2) |
| 16 | start addr (2), qty (2), byte count (1), data (`2×qty`) | start addr (2), qty (2) |
| 22 | reference addr (2), AND mask (2), OR mask (2) | echo of request |
| 23 | read addr (2), read qty (2), write addr (2), write qty (2), write byte count (1), write data (`2×write qty`) | byte count (1), data (`2×read qty`) |
| 24 | FIFO pointer addr (2) | byte count (**2**), FIFO count (2), data (`2×FIFO count`) |

Bit packing (FR-R-024): bit *n* of the range occupies bit `n mod 8` of data byte
`n / 8`; the least significant bit of the first data byte is the first coil.
Unused high bits of the final byte are zero.

Function code 24 is the only one whose byte count is two bytes wide, and it
counts the FIFO count field as well as the data (`2×FIFO count + 2`).

### 1.2 File record access

| Code | Request body | Response body |
|---|---|---|
| 20 | byte count (1), then *N* sub-requests | data length (1), then *N* sub-responses |
| 21 | data length (1), then *N* sub-requests | echo of request |

- **FC20 sub-request** (7 bytes, fixed): reference type (1) = 6, file number (2),
  record number (2), record length in registers (2).
- **FC20 sub-response**: file response length (1) = record data bytes + 1,
  reference type (1) = 6, record data (`2×record length`).
- **FC21 sub-request**: reference type (1) = 6, file number (2), record number
  (2), record length in registers (2), record data (`2×record length`).

### 1.3 Serial-line diagnostics

| Code | Request body | Response body |
|---|---|---|
| 7 | *(empty)* | output data (1) |
| 8 | sub-function (2), data (`2×n`, n ≥ 0) | sub-function (2), data (`2×n`) |
| 11 | *(empty)* | status (2), event count (2) |
| 12 | *(empty)* | byte count (1) = events + 6, status (2), event count (2), message count (2), events (0–64) |
| 17 | *(empty)* | byte count (1), server id (device-specific), run indicator (1) = `0x00` \| `0xFF`, additional data — the interior is carried, not parsed (FR-R-067) |

Status word (FR-R-068): `0xFFFF` while busy with a program function, `0x0000`
otherwise; other values are carried, not rejected.

### 1.4 Encapsulated Interface Transport (code 43)

Both directions open with MEI type (1).

| MEI | Request body | Response body |
|---|---|---|
| 13 | opaque (all remaining bytes) | opaque (all remaining bytes) |
| 14 | read device id code (1) = 1–4, object id (1) | read device id code (1), conformity level (1), more follows (1) = `0x00` \| `0xFF`, next object id (1), object count (1), objects |
| *other* | opaque (all remaining bytes) | opaque (all remaining bytes) |

- **Object** (MEI 14 response): object id (1), object length (1), object value
  (`object length`).

### 1.5 Exception response

| Field | Width |
|---|---|
| function code \| `0x80` | 1 |
| exception code | 1 |

Total PDU length is exactly 2 (FR-R-085).

---

## 2. RTU ADU

| Field | Width |
|---|---|
| address | 1 |
| PDU | 1–253 |
| CRC | 2 |

CRC-16 parameters (FR-R-092 … FR-R-094):

| Parameter | Value |
|---|---|
| Polynomial (reversed) | `0xA001` |
| Initial value | `0xFFFF` |
| Covered bytes | address + entire PDU, nothing else |
| Transmission order | low byte, then high byte |

---

## 3. ASCII ADU

| Field | Characters |
|---|---|
| start | 1 (`:`, `0x3A`) |
| address | 2 |
| PDU | 2–506 |
| LRC | 2 |
| terminator | 2 (CR LF, `0x0D 0x0A`) |

Each byte becomes exactly two ASCII hexadecimal characters, most significant
nibble first (FR-R-111). Encoding emits uppercase; decoding accepts either case
(FR-R-112).

LRC (FR-R-114): the two's complement of the 8-bit sum of the **decoded** address
byte and every decoded PDU byte — `LRC = (0x100 - (sum & 0xFF)) & 0xFF`. It is
never computed over the hexadecimal characters themselves.

---

## 4. TCP ADU

| Field | Width |
|---|---|
| transaction identifier | 2 |
| protocol identifier | 2 (always 0) |
| length | 2 |
| unit identifier | 1 |
| PDU | 1–253 |

The length field counts the bytes that follow it: the unit identifier plus the
PDU, i.e. `PDU length + 1` (FR-R-103), giving a valid range of 2–254.

---

## 5. Ranges and limits

| Limit | Value |
|---|---|
| Max PDU | 253 bytes |
| Max RTU ADU | 256 bytes |
| Max ASCII ADU | 513 characters (255 encoded bytes) |
| Max TCP ADU | 260 bytes |
| Read Coils, Read Discrete Inputs qty | 1–2000 |
| Read Holding, Read Input Registers qty | 1–125 |
| Write Multiple Coils qty | 1–1968 |
| Write Multiple Registers qty | 1–123 |
| FC23 read qty / write qty | 1–125 / 1–121 |
| FC24 FIFO count | 0–31 |
| FC20 request byte count | 7–245, exact multiple of 7 |
| FC20 response data length | 7–245 |
| FC21 request data length | 9–251 |
| File number / record number | 1–65535 / 0–9999 |
| Read device id code | 1–4 |
| Get Comm Event Log events | 0–64 bytes |
| MBAP length field | 2–254 |
| Serial address | 0 broadcast, 1–247 individual, 248–255 carried |
| Address space per register table | 0–65535 |
