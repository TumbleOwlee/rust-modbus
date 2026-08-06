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

Anything not listed below is expected to carry a citing test.

**Kind 1 — design posture, platform, toolchain, versioning.** Every
non-functional requirement except `NF-R-009`, `NF-R-012` and `NF-R-014`, which
*are* pinned — `NF-R-009` by the allocation counts in `tests/allocation.rs`, the
other two by the property tests in `tests/robustness.rs`. Each of the rest states a fact
about the build, the toolchain, or the release process, and names its enforcement
point per `NF-R-021`:

| Requirements | Enforced by |
|---|---|
| `NF-R-001`, `NF-R-002` | The `no_std`/feature attributes in `src/lib.rs`, and the bare-metal CI job |
| `NF-R-003` | The `bare-metal` CI job |
| `NF-R-004` | The comment above `[dependencies]` in `Cargo.toml` |
| `NF-R-005`, `NF-R-007` | `rust-version` in `Cargo.toml` and the `msrv` CI job |
| `NF-R-006` | `rust-toolchain.toml` |
| `NF-R-008`, `NF-R-010` | Design posture. No benchmark gates CI, by decision (`NF-R-010`) |
| `NF-R-011` | The `cfg_attr` pair in `src/lib.rs`: `forbid(unsafe_code)` when `rs485` is off, `deny(unsafe_code)` when it is on |
| `NF-R-013` | `[lints.clippy]` in `Cargo.toml`, `clippy.toml`, and the `clippy` CI job |
| `NF-R-015` | `deny.toml` and the `deny` CI job |
| `NF-R-016`, `NF-R-017`, `NF-R-018`, `NF-R-019` | Release process, `CHANGELOG.md`, and review. `NF-R-018`'s "every combination compiles" half is checked by the `features` and `bare-metal` CI jobs; what a feature may *mean* is a review judgment |
| `NF-R-020`, `NF-R-021`, `NF-R-023`, `NF-R-024` | Conventions on the test suite itself; a test cannot assert its own naming or its own port choice |
| `NF-R-022` | The `coverage` CI job |
| `NF-R-025` | The `serde` feature declaration in `Cargo.toml`, whose comment cites it, and the `features` and `no-std` CI jobs |
| `CL-R-039` | Design posture: an API that does not exist. Enforced by the absence of a probe method in `client/api-contract.md` and by review |
| `TR-R-061` | `deny.toml`'s `[bans]` `deny` list (`native-tls`, `openssl`, `openssl-sys`, `boring`, `boring-sys`) and the `deny` CI job |
| `TR-R-064` | Structural claim about *when* the TLS handshake runs relative to `FrameTransport` construction; verified by code inspection at review, not a runtime assertion |

**Kind 2 — cross-cutting restatements asserted under the owning area:**

| Requirement | Asserted under |
|---|---|
| `FR-R-120` | Each framing's own requirements — `FR-R-091`, `FR-R-104`, `FR-R-113` pin the maximum lengths and both directions per framing |
| `CL-R-003` | The framing requirements that put the identifier on the wire: `FR-R-096`, `FR-R-101`, `FR-R-117` |
| `SV-R-005` | Structural: nothing to test is the point. Recorded in `server/data-contract.md`, and a shipped data model would be a visible addition to `server/api-contract.md` |
| `SV-R-006` | The `std` gate on the server module, checked by the bare-metal CI job, whose comment cites it |
| `TR-R-032` | The `rtu` feature declaration in `Cargo.toml`, whose comment cites it, and the `features` CI job |
| `TR-R-051` | The `rs485` feature declaration in `Cargo.toml`, whose comment cites it, and the `features` CI job |
| `TR-R-055` | The `cfg_attr` pair in `src/lib.rs` and the `#[allow(unsafe_code)]` block in `src/transport/rs485.rs`, both commented; verified manually per the RS-485 implementation plan that a second, unrelated unsafe block is still rejected with `rs485` enabled |

## Keeping specs true

Before changing code in an area, read that area's `requirements.md`. If the
change contradicts the spec, update the spec **in the same commit**. A behavior
change with no spec change is an incomplete change.
