# rust-modbus Specs

Authoritative specification of `rust-modbus`'s behavior, split by capability area.

These files are **normative**: the code is expected to conform to them, not the
other way around. When code and spec disagree, that is a defect in one of them —
resolve it, don't paper over it.

## Areas

| Area | Covers | ID prefix |
|---|---|---|
| [`frame/`](./frame/) | PDU and ADU encode/decode, function codes, exception responses, CRC-16 (RTU), MBAP header (TCP) | `FR-R-nnn` |
| [`client/`](./client/) | Async client API, request issuing, response matching, timeouts, retry and reconnect | `CL-R-nnn` |
| [`server/`](./server/) | Async server, connection handling, request dispatch, data store, exception generation | `SV-R-nnn` |
| [`transport/`](./transport/) | TCP sockets and RTU serial ports, framing boundaries, connection lifecycle | `TR-R-nnn` |

Cross-cutting: [`non-functional-requirements.md`](./non-functional-requirements.md)
(`NF-R-nnn`).

## Rules for writing specs

**1. No code pointers.** Never cite `file:line`, function names, struct names, or
crate-internal identifiers. A spec states *what must be true*, not where it is
implemented — code pointers rot on every refactor and turn the authoritative doc
into a liar. The **public API is different**: exported type names, method
signatures, error variants, feature flags, and configuration fields are part of
the contract and *are* spec content. They belong in the area's
`api-contract.md`.

**2. Requirement IDs are stable and append-only.** Each requirement carries an ID
from its area's prefix (see the table above). Never renumber. Never reuse a
retired ID. A deleted requirement's ID stays dead. Reference requirements by ID
in commits, PRs, tests, and agent instructions.

**3. Owner is the behavior, not the surface.** A serial baud-rate configuration
field is specified in `transport/`, not wherever it happens to be typed in — it
belongs with the behavior it controls, so one change touches one file. `frame/`
owns everything that is true of a byte sequence regardless of who sent it;
`client/` and `server/` own only role-specific behavior. If both roles must
behave identically, the requirement belongs in `frame/`, stated once.

**4. Requirements are testable.** Write "shall" statements with observable
outcomes. "The client shall fail a request with a timeout error after the
configured response timeout elapses with no matching response" is a requirement.
"The client is robust" is not. For a wire protocol, the strongest form names the
exact bytes.

**5. Known gaps are specified, not hidden.** Behavior that is ugly but
intentional (an unsupported function code, a deliberate deviation, a missing
retry policy) belongs in the area's `edge-cases.md` as a stated constraint — so
it is not mistaken for an oversight and silently "fixed".

## Per-area files

Not every area needs every file; add and drop based on need.

| File | Contains |
|---|---|
| `requirements.md` | Numbered, testable "shall" statements. Every area has one. |
| `api-contract.md` | The area's stable public surface: exported types and signatures, error variants, supported function codes, configuration fields, feature flags. |
| `data-contract.md` | Wire formats: ADU/PDU layouts, byte and word order, address ranges, field widths. |
| `edge-cases.md` | Boundary behavior, error semantics, and stated known limitations. |

## Requirements intentionally not unit-tested

Most requirements are pinned by a test whose doc comment cites the ID (`FR-R-*`,
`CL-R-*`, …). A minority are **deliberately** left without a dedicated test — they
are not gaps. This list records that decision so it is not re-discovered as one.
Two kinds qualify; nothing else does.

**1. Design-posture, platform, toolchain, and versioning statements.** These
assert facts about the build or the design, not runtime behavior a `shall` test
could observe.

**2. Cross-cutting restatements whose behavior is asserted under the owning
area.** The requirement is real, but its test lives with the per-area requirement
that owns the behavior, cited by *that* ID.

*(Populate this list as such requirements appear. Anything not on it is expected
to carry a citing test.)*

## Keeping specs true

Before changing code in an area, read that area's `requirements.md`. If the
change contradicts the spec, update the spec **in the same commit**. A behavior
change with no spec change is an incomplete change.
