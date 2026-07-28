# Non-Functional Requirements

Cross-cutting requirements that belong to no single capability area: platforms,
toolchain, performance posture, security, versioning, and testing conventions.

IDs are stable and append-only (`NF-R-nnn`). See [`README.md`](./README.md).

Requirements are added through the workflow in [`AGENTS.md`](../../AGENTS.md) —
gate 1 approves the "shall" text before any code is written. Nothing here is
fabricated ahead of that.

---

## 1. Platforms and toolchain

*(TBD — target platforms, MSRV, toolchain channel. The toolchain is currently
pinned in `rust-toolchain.toml`; state it normatively here when the MSRV policy
is decided.)*

---

## 2. Performance

*(TBD — the posture on allocations per frame, throughput expectations, and
whether any benchmark is asserted rather than merely recorded.)*

---

## 3. Security and robustness

*(TBD. The intended posture, to be written as normative text: no input from a
peer — however malformed, truncated, or oversized — may cause a panic, an
out-of-bounds access, or an unbounded allocation.)*

---

## 4. Versioning and API stability

*(TBD — semver policy, what counts as a breaking change, feature-flag stability.)*

---

## 5. Testing conventions and coverage

*(TBD. The intended content, to be written as normative text: unit tests named
`ut_*` beside the code, integration tests named `it_*` under `tests/`; every
requirement pinned by a test citing its ID except those listed as intentionally
untested; line coverage at or above 80% enforced in CI; TCP tests bind ephemeral
ports; RTU tests require no physical hardware.)*
