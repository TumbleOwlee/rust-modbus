# Non-Functional Requirements

Cross-cutting requirements that belong to no single capability area: platforms, toolchain, performance posture, security, versioning, and testing conventions.

IDs are stable and append-only (`NF-R-nnn`). See [`README.md`](./README.md).

Requirements are added through the workflow in [`AGENTS.md`](../../AGENTS.md) — gate 1 approves the normative text before any code is written. Nothing here is fabricated ahead of that.

---

## Platforms and toolchain

**NF-R-001** — The crate builds for `no_std` targets with `alloc` available. The frame area depends on `core` and `alloc` only.

**NF-R-002** — The crate exposes a `std` feature, enabled by default. The client, server, and transport areas require it; the frame area does not.

**NF-R-003** — CI builds the crate for a bare-metal target (`thumbv7em-none-eabi`) with default features disabled, so a dependency on `std` cannot be reintroduced unnoticed.

**NF-R-004** — Dependencies are declared with `default-features = false`, enabling only the features the crate uses.

**NF-R-005** — The crate's minimum supported Rust version is **1.88.0**. It is declared as `rust-version` in the package manifest, so a consumer on an older toolchain is told the requirement by Cargo rather than by a compile error inside the crate. CI builds and tests the crate on exactly that version, so the declared MSRV is verified rather than asserted.

**NF-R-006** — The development toolchain is pinned to the `stable` channel in `rust-toolchain.toml`, with the `rustfmt`, `clippy`, and `llvm-tools-preview` components, so every contributor's formatting, lint, and coverage results agree with CI's. The pinned development toolchain is the newest supported version, not the oldest; the MSRV of NF-R-005 is a separate, lower floor.

**NF-R-007** — Raising the MSRV is a normative change: it requires a spec change to NF-R-005, a matching `rust-version` bump, and a `CHANGELOG.md` entry. The MSRV never rises as an incidental consequence of a dependency update.

---

## Performance

**NF-R-008** — Memory used to hold a single frame is bounded by a compile-time constant per framing — the framing's maximum ADU length — and does not depend on values a peer controls. Neither a decoder nor a transport allocates a buffer sized from a length field that has not yet been validated against that maximum.

**NF-R-009** — Encoding a frame allocates nothing in steady state. Every encode path appends into a caller-supplied buffer whose capacity was reserved before writing began (FR-R-140, FR-R-141), and a transport reuses one such buffer across frames (TR-R-043). Once a transport has sent its first frame, sending allocates zero times; a caller that asks for an owned `Vec` instead pays exactly one allocation, by its own choice. No buffer is sized from a value a peer controls.

**NF-R-010** — The crate asserts **no** throughput, latency, or allocation-count figure. It ships no benchmark suite, and no performance number gates CI. Performance is a design posture — bounded, allocation-conscious frame handling per NF-R-008 and NF-R-009 — and a performance regression that keeps behavior correct is not caught automatically. Introducing a benchmark that CI asserts on is a scope decision, not an incremental addition.

---

## Security and robustness

**NF-R-011** — The crate contains no `unsafe` code under its default feature set, and under any feature combination that excludes `rs485`; the compiler enforces this through a crate-level `forbid(unsafe_code)` attribute active whenever `rs485` is not enabled. Enabling the off-by-default `rs485` feature (TR-R-051) narrows this to `deny(unsafe_code)`, admitting exactly one `#[allow(unsafe_code)]` block: the single, documented `TIOCSRS485` ioctl call, gated to `target_os = "linux"` (TR-R-050, TR-R-055). Every other unsafe block — in this crate today or introduced by a future change — remains denied and fails the build.

**NF-R-012** — No input from a peer — however malformed, truncated, oversized, or deliberately hostile — causes a panic, an out-of-bounds access, or an unbounded allocation. Every such input instead produces a typed error variant. This holds for every decode path in the crate, at both the PDU and the ADU layer, and for every framing.

**NF-R-013** — Non-test code denies the lints that make NF-R-012 reachable by accident: `clippy::unwrap_used`, `clippy::indexing_slicing`, and `clippy::panic`. Every fallible call in non-test code documents why it cannot fail via `expect("...")`. Test code is exempt from all three, since a panicking assertion there is the test. CI runs Clippy with warnings denied over all targets and all features.

**NF-R-014** — NF-R-012 is pinned by property-based tests over generated input, not by a fixture list alone: arbitrary byte sequences spanning lengths from empty to past the largest ADU any framing permits, byte sequences drawn from the ASCII framing's own alphabet so generation reaches past the hexadecimal check, and every truncation prefix of a valid ADU. These tests run as part of the default test suite, and any counterexample the generator finds is committed as a regression seed.

**NF-R-015** — CI audits the dependency tree on every push and pull request for known security advisories, for licences outside an explicit permissive allow-list, and for source registries outside crates.io. The audit fails the build, not merely reports. Its configuration records why each non-standard licence in the tree is accepted.

---

## Versioning and API stability

**NF-R-016** — The crate is versioned according to Semantic Versioning. While the major version is 0, a breaking change bumps the minor version and an additive or fixing change bumps the patch version.

**NF-R-017** — The following count as breaking changes: removing or renaming any publicly exported item; changing the signature, generic parameters, or trait bounds of a public function or trait method; adding, removing, or reordering the variants or fields of a public enum or struct; changing a public type's field types; removing a feature flag or changing what one enables; and raising the MSRV per NF-R-007. Public enums and structs in this crate are **exhaustive** — none carries `#[non_exhaustive]` — so adding a variant to the error enum, or a field to a configuration struct, is a breaking change and not an additive one.

**NF-R-018** — Feature flags are purely additive. Enabling a feature only adds public API; it never removes an item, changes a signature, or alters the behavior of anything available without it. No feature is mutually exclusive with another, and any combination of features compiles.

**NF-R-019** — The repository maintains a `CHANGELOG.md` recording, per released version, the added, changed, and removed public API, every breaking change, and every MSRV change. Unreleased work accumulates under an `Unreleased` heading.

---

## Testing conventions and coverage

**NF-R-020** — Unit tests live in a `#[cfg(test)] mod tests` block at the bottom of the file under test, with function names prefixed `ut_`. Integration tests live in files under `tests/`, with function names prefixed `it_`. The prefix distinguishes the two in a single test run's output, so a failure names its own scope.

**NF-R-021** — Every requirement in `docs/specs/` is pinned by at least one test whose doc comment cites the requirement's ID, placed directly below the `#[test]` or `#[tokio::test]` attribute. The sole exceptions are the requirements listed under `## Requirements intentionally not unit-tested` in [`README.md`](./README.md); a requirement absent from both that list and the test suite is a gap. A requirement enforced structurally rather than by a test — by the compiler, by a lint, or by a CI job — names its ID in a comment in the manifest, lint configuration, or workflow that enforces it, so the enforcement point is discoverable from the ID.

**NF-R-022** — Line coverage is at or above **80%**, measured by `cargo llvm-cov` over all features and enforced with `--fail-under-lines 80` as a CI job on every push and every pull request. The floor is a floor, not a target: it catches an untested module, it does not certify a tested one. Tests that execute code without asserting on its result are never added to raise it.

**NF-R-023** — Any test that binds a TCP listener binds port 0 and reads the assigned port back; no test names a fixed port number. A test that deliberately occupies a port to exercise bind-failure handling binds the occupying listener ephemerally first, then points the subject at the port it was given.

**NF-R-024** — No test that runs by default requires physical hardware, a serial device node, or an externally launched process. Serial and stream behavior is exercised over an in-memory duplex pair. A test that requires a real `/dev/tty*` device or a separately started external Modbus endpoint is marked `#[ignore]` with a reason naming what it needs, so it is opt-in and never runs in CI.

**NF-R-025** — The crate exposes an off-by-default `serde` feature gating the `Serialize`/`Deserialize` implementations named by FR-R-151, CL-R-065, SV-R-054, TR-R-058 and TR-R-059. The `serde` dependency is declared `default-features = false` with exactly the `derive` and `alloc` features enabled, so enabling `serde` never pulls in `std` on a build that has excluded it (NF-R-001, NF-R-002) and never enables any serde data format as a transitive dependency. CI builds the bare-metal target of NF-R-003 with `serde` enabled and `std` excluded, in addition to the existing no-feature build.

**NF-R-026** — The crate exposes an off-by-default `sync` feature gating the blocking client of CL-R-070. It implies `std` and is therefore never present in the bare-metal build of NF-R-003. Per NF-R-018 it is purely additive: enabling it changes nothing about the async client. CI builds and tests with it enabled.

**NF-R-027** — The crate exposes an off-by-default `tls` feature gating TR-R-060, TR-R-061, TR-R-062, TR-R-063, TR-R-064, TR-R-065, TR-R-066, TR-R-067, TR-R-068, TR-R-096 and `Connection::peer_cert` (SV-R-055). Implies `std`, absent from bare-metal build (NF-R-003). Depends on `rustls`, `tokio-rustls`, `rustls-pki-types` (also PEM parsing, via its `PemObject` trait), and `rustls-native-certs` (populates `ServerCertVerification::Verify`'s default platform-native root store, TR-R-065), each `default-features = false`.
