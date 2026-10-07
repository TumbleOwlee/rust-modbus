# rust-modbus Specs

Authoritative spec of `rust-modbus`'s behavior, by capability area. **Normative**: code conforms to these, not vice versa. Code/spec disagreement = defect in one — resolve, don't paper over. Areas, ID prefixes and the rules of engagement live once, in the agent-instructions file's routing table (`AGENTS.md`, `## Where to look for task X`) — this file never carries a second copy. Cross-cutting: [`non-functional-requirements.md`](./non-functional-requirements.md) (`NF-R-nnn`).

## Rules for writing specs

1. **No code pointers.** Never cite file:line, function/type names, internal identifiers — specs state *what must be true*, not where implemented (code pointers rot on refactor). Exception: public, user-facing surface (exported names, signatures, error variants, feature flags, config fields, CLI flags, commands) IS spec content → area's `api-contract.md`.
2. **IDs stable, append-only.** `<PREFIX>-R-nnn` for a requirement, `<PREFIX>-E-nnn` for an `edge-cases.md` entry; prefix per area from `AGENTS.md`'s routing table (`NF` has no `-E` series). The two series are numbered independently per area; never renumber, never reuse a retired ID (a deleted entry's ID stays dead). Sole exception: an ID defining two different entries, which no other repair can make unique — the definition with fewer citations in the sources takes the next free ID in its series, keeping its wording and its position in the file, and every citation of it is updated in the same change; the ID that was duplicated keeps the other definition. Reference by ID in commits/PRs/tests/agent instructions. An `-E` entry derived from a requirement cites that `-R` ID in its text and still carries its own `-E` ID. Tests cite `-E` IDs exactly as `-R` IDs (rule 8).
3. **Owner = the behavior, not the surface.** Spec a config field with the behavior it controls, not where it's typed (one change → one file). Shared behavior across areas → belongs to the area owning it, stated once.
4. **Requirements are testable.** Normative indicative statements, observable outcomes — no "shall"/"must"/"should": every entry is normative by being here, the modal verb is noise. Good: "The client fails a request with a timeout error after the configured response timeout elapses with no matching response." Bad: "The client is robust." Format-involving requirements: name the exact bytes where possible.
5. **Known gaps are specified, not hidden.** Intentional-but-ugly behavior (unsupported case, deliberate deviation, missing policy) → area's `edge-cases.md` as a stated constraint with its own `-E` ID, so it isn't mistaken for an oversight and "fixed".
6. **One requirement or edge-case entry, one physical line.** No line break inside a `**<PREFIX>-R-nnn** — ...` or `**<PREFIX>-E-nnn** — ...` statement, however long, table rows included — `grep -rn <ID> docs/specs/` (or any keyword from the text) must return the whole entry in a single match, not a truncated first line. Exact file:line by ID: `sh .claude/scripts/extract-id.sh <ID> [<ID> ...]` (searches `docs/specs` by default; batch every ID needed into one call). One section instead of a whole file: `sh .claude/scripts/extract-section.sh '## <heading>' docs/specs/<area>/requirements.md`. Headings are unnumbered: `extract-section.sh` matches heading text verbatim, and a number churns every reference when a section is inserted.
7. **Contract-file citations.** `api-contract.md`/`data-contract.md` carry no ID series of their own; every table row carries a `Req` column naming the owning requirement ID(s), prose entries cite inline. IDs are always enumerated, never given as a range. A row with no owning requirement is marked `—` and raised as a missing requirement, never given an invented ID. Cross-references between spec files use an `-R`/`-E` ID when the target has one, and the target's exact heading text (the string `extract-section.sh` accepts) otherwise — section numbers (`§n.m`) are not used.
8. **Cite every ID a test pins, and only those.** A test's doc comment lists every requirement or edge-case ID its assertions directly verify, at least one, comma-separated before the ` — ` summary (`/// FR-R-012, FR-E-003 — …`). An ID the test merely exercises on the way without asserting its outcome is not cited. A test citing many IDs is a review prompt that it does too much, not a violation. `grep -rn <ID>` over the sources therefore returns every test that pins that rule.
9. **One entry, one rule.** A requirement states exactly one observable rule: one subject, one condition, one outcome. A statement that needs "and also", "additionally", a second sentence introducing a different subject, or an enumeration of independent behaviors is several requirements and gets one ID each, cross-citing where one depends on another. Test: can a single test pin the whole entry, and can the entry be falsified by one counterexample? If not, split before landing.

## Per-area files

Not every area needs every file — add/drop per need.

| File | Contains |
|---|---|
| `requirements.md` | Numbered, testable normative statements, one rule each (rule 9). Every area has one. |
| `api-contract.md` | Stable public surface: exported names/signatures, error variants, config fields, feature flags, CLI flags, commands. No ID series of its own; each table row's `Req` column names the owning requirement ID(s). |
| `data-contract.md` | Formats: wire layouts, file schemas, field widths, ordering, ranges. No ID series of its own; each table row's `Req` column names the owning requirement ID(s). |
| `edge-cases.md` | Boundary behavior, error semantics, stated known limitations. Entries carry their own `-E-nnn` IDs. |

## Requirements intentionally not unit-tested

Most requirements are pinned by a test whose doc comment cites the ID. A minority are **deliberately** untested — not gaps; this list records that decision. Four kinds qualify:

1. **Design-posture/platform/toolchain/versioning statements** — assert facts about build/design, not runtime behavior a test could observe. Each names its enforcement point (CI job, manifest field, lint config) instead.
2. **Cross-cutting restatements whose behavior is asserted under the owning area** — requirement is real, but its test lives with the per-area requirement that owns the behavior, cited by *that* ID.
3. **Structural/shape requirements collectively exercised by one round-trip or property test, not per-requirement** — a group of requirements describing the shape of a data structure (which fields exist, defaults, omission rules), covered by a single save→load→compare or property test; behavioral requirements around them tested on their own.
4. **Stated known limitations (`-E` entries)** — class-wide exemption, rules under Kind 4 below.

Anything not listed below, and no `-E` entry outside kind 4, must carry a citing test. A listed requirement that gains observable behavior: remove it from the list, add the test. List only the kinds that have entries; drop an empty subsection rather than leaving a placeholder row.

**Kind 1 — design posture/platform/toolchain/versioning**

Every non-functional requirement except `NF-R-009`, `NF-R-012` and `NF-R-014`, which *are* pinned — `NF-R-009` by the allocation counts in `tests/allocation.rs`, the other two by the property tests in `tests/robustness.rs`. Each of the rest names its enforcement point per `NF-R-021`:

| Requirement | Enforced by |
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
| `CL-R-079` | Design posture: an API that does not exist. Enforced by the absence of a blocking server in `client/api-contract.md` and `server/api-contract.md`, and by review |
| `SV-R-063` | Structural: every `ServerConfig` field's default is listed in `server/api-contract.md`; checked at review |
| `SV-R-070` | Design posture: no bind method exists in `server/api-contract.md`; checked at review |
| `SV-R-072` | Structural: no server-owned variant in the `Error` enum (`frame/api-contract.md` `## Error variants`); checked at review |
| `TR-R-061` | `deny.toml`'s `[bans]` `deny` list (`native-tls`, `openssl`, `openssl-sys`, `boring`, `boring-sys`) and the `deny` CI job |
| `SV-R-069` | The `features` CI job (`cargo check` with and without `rtu`); the server module carries no `rtu` gate, checked at review |
| `TR-R-064` | Structural claim about *when* the TLS handshake runs relative to `FrameTransport` construction; verified by code inspection at review, not a runtime assertion |

**Kind 2 — cross-cutting restatements**

| Requirement | Asserted under |
|---|---|
| `FR-R-120` | Each framing's own requirements — `FR-R-091`, `FR-R-104`, `FR-R-113` pin the maximum lengths and both directions per framing |
| `CL-R-003` | The framing requirements that put the identifier on the wire: `FR-R-096`, `FR-R-101`, `FR-R-117` |
| `SV-R-005` | Structural: nothing to test is the point. Recorded in `server/data-contract.md`, and a shipped data model would be a visible addition to `server/api-contract.md` |
| `SV-R-006` | The `std` gate on the server module, checked by the bare-metal CI job, whose comment cites it |
| `TR-R-032` | The `rtu` feature declaration in `Cargo.toml`, whose comment cites it, and the `features` CI job |
| `CL-R-094` | The `pipeline` feature declaration in `Cargo.toml`, whose comment cites it, and the `features` CI job |
| `TR-R-060` | The `tls` feature declaration in `Cargo.toml`, whose comment cites it, and the `features` CI job |
| `TR-R-051` | The `rs485` feature declaration in `Cargo.toml`, whose comment cites it, and the `features` CI job |
| `TR-R-055` | The `cfg_attr` pair in `src/lib.rs` and the `#[allow(unsafe_code)]` block in `src/transport/rs485.rs`, both commented; verified manually per the RS-485 implementation plan that a second, unrelated unsafe block is still rejected with `rs485` enabled |

**Kind 4 — stated known limitations (`-E` entries)**

No ID table — this kind is a class-wide exemption, not an enumerated list. An `-E` entry that records a known limitation or an observed constraint rather than an asserted behavior is exempt as a class. An `-E` entry that does assert behavior (most boundary-table rows) is outside the exemption and wants a citing test.
