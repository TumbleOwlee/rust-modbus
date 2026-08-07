# Frame — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**,
for the core area: PDU structure, function code taxonomy, exception responses,
buffer reuse, and serde/`Display`. Data-access and ADU-specific edge cases live
in [`../frame-data-access/edge-cases.md`](../frame-data-access/edge-cases.md)
and [`../frame-adu/edge-cases.md`](../frame-adu/edge-cases.md).

Everything in §5 is working as specified; it is recorded here so it is not
mistaken for an oversight and silently "fixed".

---

## 1. PDU decode boundaries

| Condition | Behavior |
|---|---|
| Empty input | truncated-input error (FR-R-131) |
| PDU shorter than its layout requires | truncated-input error naming expected vs. supplied (FR-R-131) |
| PDU longer than its layout requires | trailing-bytes error (FR-R-132) |
| Function code 0 | invalid-function-code error (FR-R-014) |
| Function code 128–255 in the request direction | invalid-function-code error (FR-R-015) |
| Function code 1–127 that the crate does not name | decodes as `Custom(u8)`, body opaque (FR-R-011, FR-R-012) |
| Trailing bytes after a `Custom` body | none possible — the body is defined as all remaining bytes (FR-R-132) |
| Encoding `Custom(u8)` with a named code | reserved-code error (FR-R-013) |

## 2. Exception decode boundaries

| Condition | Behavior |
|---|---|
| Response function code with the high bit set | decoded as an exception response, not an error (FR-R-081) |
| Exception code outside the nine named, including 0 | decodes successfully as a general exception value (FR-R-083) |
| Encoding a general exception value holding a named code | reserved-code error (FR-R-084) |
| Exception response PDU of length ≠ 2 | length error (FR-R-085) |
| Exception response for a `Custom` function code | decodes normally; the exception path does not depend on the code being named (FR-R-086) |

Every row above is an error, never a panic: FR-R-130 admits no exception for any
input whatsoever.

## 3. Encode buffers

| Condition | Behavior |
|---|---|
| `encode_into` fails part-way | The buffer is truncated back to its length on entry; nothing partial is left behind (FR-R-142) |
| `encode_into` on a buffer that already holds bytes | Appends after them; when to clear is the caller's decision, not the frame layer's (FR-R-140) |
| `encode_into` on a buffer with no spare capacity | Reserves before writing, so the allocation happens once at the top rather than repeatedly beneath (FR-R-141) |
| A PDU that would exceed `MAX_PDU_LEN` | Too-large error, measured against the bytes *this* call wrote, not the buffer's total length (FR-R-002) |
| ASCII appending encode | Uses one scratch buffer per frame; RTU and TCP use none (FR-R-143) |
| `encode` (the allocating form) | One allocation, sized on the framing maximum, then the appending path unchanged (FR-R-140) |

## 4. Serde and Display

| Condition | Behavior |
|---|---|
| Deserializing a value the protocol would reject — `UnitId(250)`, `Quantity(2001)` | Succeeds. The wrapped integer's width is the only bound; anything wider simply fails to parse as that integer. Deserialize adds no validation the constructor does not already skip (FR-R-007, FR-R-151) |
| `Display` of a value no function code would accept | Renders like any other value; `format!("{}", UnitId(250))` is `"250"`. Legality belongs to encoding (FR-R-021 and friends), not to formatting |
| `Display` of `FunctionCode::Custom` or `ExceptionCode::Other` | Always carries the number, since there is no name to substitute — the one place these impls differ from the named case, which never shows a number |
| The serde representation as a compatibility surface | Changing a domain type's wrapped width, or a config field's serde name or unit, changes what an already-stored TOML or JSON file parses as, independently of whether the Rust API broke. NF-R-017 did not previously need to consider this, because nothing was serializable |

## 5. Known limitations

- **Custom codes carry no semantics.** `Custom(u8)` preserves bytes; it does not
  know quantities, addresses, or lengths, so nothing beyond the PDU size limit is
  validated. A consumer using vendor codes owns their meaning.
- **Domain value types validate nothing.** `Address`, `Quantity`, `UnitId` and
  the rest (FR-R-007) are transparent wrappers: every value the field's width can
  hold is constructible, including `UnitId(250)` from the reserved range and a
  `Quantity` no function code accepts. They prevent swapping two fields of the
  same width, not choosing a bad value — the range rules stay where they already
  are, in encoding (FR-R-021, FR-R-027, FR-R-031) and in the server's judgment.
- **The frame layer validates no address against any device map.** Structural
  validity only; Illegal Data Address is the server area's judgment.
