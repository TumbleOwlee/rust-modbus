# Contributing to rust-modbus

Thanks for your interest in contributing! This document covers the essentials to
get you productive quickly.

## Setup

`rust-modbus` is written in Rust (stable toolchain, pinned via
`rust-toolchain.toml`). Install the toolchain via [rustup.rs](https://rustup.rs/),
then:

```sh
git clone <your-fork>
cd rust-modbus
cargo build --all-features
```

For the coverage gate you also need `cargo-llvm-cov`:

```sh
cargo install cargo-llvm-cov --locked
```

Optionally install [lefthook](https://github.com/evilmartians/lefthook) and run
`lefthook install` to get the pre-commit `fmt`/`clippy` checks locally.

## Project Layout

A single library crate. See [`ARCHITECTURE.md`](./ARCHITECTURE.md) for the module
map and data flow, and [`PRD.md`](./PRD.md) for the product framing.

`rust-modbus` is **spec-driven**: [`docs/specs/`](./docs/specs/) is the
authoritative specification of what the library must do, split by capability
area (`frame/`, `client/`, `server/`, `transport/`). The code is expected to
conform to it. Before changing behavior, read the relevant area's
`requirements.md`.

## Test-Driven Development

Write the test first, watch it fail, then implement. A test written after the
code it covers asserts what you built rather than what the Modbus standard
requires — for wire formats especially, derive expected bytes from the
specification, not from a debug print of your own encoder.

Every new or changed requirement ships with at least one test whose doc comment
cites the requirement ID, on the line directly below the `#[test]` attribute:

```rust
#[test]
/// FR-R-012 — CRC-16 is computed over the full ADU excluding the CRC field itself.
fn ut_crc_excludes_trailer() { /* … */ }
```

Line coverage must stay at or above **80%**, enforced in CI. Coverage is a floor,
not a goal — never pad it with tests that execute code without asserting on it.

## Before Submitting

Please make sure the following pass locally:

```sh
cargo fmt --check
cargo clippy --all-features -- -D warnings
cargo check --all-features
cargo test --all-features
cargo llvm-cov --all-features --fail-under-lines 80
```

CI runs all five as separate steps — on every push **and every pull request** — so
anything the pre-commit hook would reject is rejected by CI too.

## Pull Requests

- Branch off `main` and open your PR against `main`. Branch naming:
  `<type>/<slug>` with a conventional-commit type (`feat/`, `fix/`, `docs/`).
- Keep PRs focused — one feature or fix per PR.
- Add or update tests for behavior changes; unit tests live in `#[cfg(test)]`
  modules next to the code (`ut_*`), integration tests in `tests/` (`it_*`).
- **Update the spec in the same PR.** When you change behavior, update the
  relevant `docs/specs/<area>/` file(s) — they are the authoritative source, not a
  one-time snapshot. New requirements get a fresh, appended ID (never renumber or
  reuse). A behavior change with no spec change is incomplete.
- Reference requirement IDs in the PR body, and `Closes #<issue>` for the
  tracking issue.
- Update the README when you change the public API, feature flags, or supported
  function codes.
- PRs are merged to `main` by **squash merge**.

Agents working in this repo follow the fuller gated workflow in
[`AGENTS.md`](./AGENTS.md); human contributors are welcome to, but the checks
above are the hard requirements.

## Reporting Issues

Open a GitHub issue with steps to reproduce, the `rust-modbus` version (or
commit), and your platform. For wire-level problems, a hex dump of the frames
involved (and the peer device or software on the other end) is the single most
useful thing you can include.
