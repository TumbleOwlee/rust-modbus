# AGENTS.md

Router for AI coding agents working in this repo. Read this first; it points to
everything else.

## What this repo is

`rust-modbus` — a Rust library providing **async Modbus client and server**
capabilities over **RTU** and **TCP**. A single crate (`rust-modbus`) organised
into modules; no binary is shipped, the crate is a library consumed by other
projects. Product framing: [`PRD.md`](./PRD.md). Structure and module map:
[`ARCHITECTURE.md`](./ARCHITECTURE.md).

## Spec-driven — read this before you change behavior

`docs/specs/` is the **authoritative** specification: the code is expected to
conform to it, not the other way around. Before you edit code in an area, read that
area's `requirements.md`. A behavior change with no spec change is incomplete — the
workflow below is how the two stay together.

**`main` never contains an unfinished spec.** A requirement on `main` is a statement
about code that exists and is tested. A feature branch may hold a spec commit ahead of
its implementation (see the workflow); `main` may not, which the squash merge
guarantees.

If the code and the spec already disagree and it is *not* what you were asked to fix:
**stop and raise it as its own task.** Do not fold the fix into the change in flight —
it silently widens work that was already approved, and the fix deserves its own review.

Specs contain no `file:line` pointers by design — locate code with your own search
tools. Requirements have stable IDs (`FR-R-*`, `CL-R-*`, `SV-R-*`, `TR-R-*`,
`NF-R-*`); reference them in commits and PRs.

## Test-driven — the test comes before the implementation

This library is **test-driven**, not merely tested. Within a stage the order is
fixed:

1. Write the test that pins the requirement. Its doc comment cites the requirement
   ID (`/// FR-R-012 — …`).
2. **Run it and watch it fail** for the right reason — a wrong assertion, a
   compile error in the test itself, or a test that passes before the code exists
   is a test that proves nothing. Report what the failure was.
3. Write the minimum implementation that makes it pass.
4. Refactor with the test green.

A stage that adds implementation without a preceding failing test is not done, and
neither is a stage whose test was written afterwards to fit the code. Wire protocols
are exactly where after-the-fact tests rot: a test written against the encoder you
just wrote asserts what you built, not what Modbus requires. Where the wire format
is specified by the Modbus standard, derive the expected bytes from the spec
document, not from a debug print of your own output.

**Coverage floor: 80% of lines, enforced in CI.** `cargo llvm-cov --fail-under-lines
80` gates every push and pull request. Coverage is a floor, not a target — it catches
untested modules, it does not certify the tested ones. Never chase the number with
tests that execute code without asserting on it.

## Workflow — follow this for every behavior change

This project's workflow **replaces** any generic workflow skill (including `/workflow`);
do not run one here. `docs/specs/` already serves as the PRD and the design record — a
second design-artifact system would only give the "why" two homes to diverge in.

**It triggers on behavior change, not on size.** Ask: *does this change what the
software is required to do?* If yes — a new function code, a changed timeout default,
a different error variant, any observable API semantics — the full workflow applies,
however small the diff. If no — a refactor, a rename, perf work with identical
semantics, tests, docs — there is no spec diff to approve, so skip the gates and just
do the work. Size decides how many *stages* the plan has, never whether the gates exist.

Work on a branch off `main`, never on `main` itself. `<type>/<slug>`, conventional-commit
type (`feat/`, `fix/`, `docs/`).

1. **Read the affected area's spec.** Use the routing table below to find it. Read
   `requirements.md` and `edge-cases.md` before proposing anything — `edge-cases.md`
   records behavior that is ugly *on purpose*.
2. **Gate 1 — the behavior contract.** Propose the **spec diff itself**: the actual
   "shall" text of the new or changed requirements, with their appended IDs, plus any
   `edge-cases.md` entries. Not prose about what you intend to build — the normative
   text, ready to land. Design choices that are observable *are* spec, and get settled
   here; for a library, that includes the public type and function signatures, the
   error enum, and feature-flag gating. **Stop for approval.** For a bug fix where the
   spec is already right and the code is wrong, there is no diff to approve: state the
   requirement the code violates and move on.
3. **Gate 1b — the tracking issue.** Once the spec is approved, search the repo's open
   issues (`gh issue list`, plus a search of closed ones) for anything with the **same
   goal**. If one exists, use it — reference its number from here on, do not open a second.
   If none exists, draft the issue title and body and **stop for approval**; create it
   with `gh issue create` only once confirmed. Give it a **human-friendly title** — a
   plain-language summary of the goal a maintainer can scan, not a slug, a requirement ID,
   or a restated commit subject.

   The issue must be **self-contained**: at this point the spec lives only in the working
   tree, so a reader who has only the issue cannot look a requirement ID up. **Always quote
   the full normative text** of every new requirement next to its ID, and list every
   *changed* requirement the same way (old → new), plus the `api-contract.md` and
   `edge-cases.md` entries. An ID with no text is useless to the reader.

   The issue body states the **goal** and the normative changes only. **Never put
   implementation detail in it** — how the code will be structured, which files or
   functions change, the chosen approach. That is part of the implementation, so it belongs
   to the plan (gate 2) and to the PR that describes how the issue was resolved, never to
   the issue.

   **Structure every issue with `##` section headers**, not a wall of prose, so a reader can
   scan it: a `## Background` (or `## Why`) stating the problem and context, a `## Scope` (or
   the requirement changes) naming what is in scope, and a `## Goal` stating the outcome.
   Add further sections as the issue warrants. Keep long enumerations compact (grouped ID
   ranges, not one paragraph per item). The same structured, header-per-section shape applies
   to PR bodies (gate 3).
4. **Write the spec into the working tree.** Do not mark it "unfinished" in the file —
   the file only ever contains normative text. The plan tracks what is not yet backed by
   a passing test.
5. **Gate 2 — the implementation plan.** Stages, file-level steps, a table mapping each
   new requirement ID to the test that will pin it, and a **Verification** section naming
   how the change will be exercised (unit tests alone; a loopback TCP integration test; a
   virtual serial pair; interop against an external Modbus master/slave). State the expected
   commits and the expected coverage impact. **Stop for approval.**
6. **Implement, stage by stage — test first.** Follow the four-step TDD order above for
   every stage. A stage is a **green checkpoint**: it compiles, `cargo test` passes,
   `cargo clippy --all-targets -- -D warnings` passes, and coverage is at or above 80%. **Commit every
   green stage** — that is what makes the plan resumable after an interrupted session.
   Stage commits are branch-local scaffolding and are squashed away on merge, so keep their
   messages cheap; the squash message is the one that must carry the requirement IDs and
   the why. The spec is the first stage, hence the first commit — legal on a branch,
   never on `main`.

   Every new or changed requirement ships with at least one test whose doc comment cites
   its ID (`/// CL-R-021 — …`). Existing tests carry IDs on the same terms: every test that
   pins observable behavior shall cite the requirement it verifies. A test of a pure internal
   or helper detail that no requirement governs may stay untagged. Where a test verifies real
   behavior that no requirement yet states, add the requirement (a normative change — gate 1)
   rather than attach a loose ID.

   The citing doc comment goes on the line **directly below** the `#[test]`/`#[tokio::test]`
   attribute, immediately above the `fn`. A given requirement ID appears **at most once** per
   test — one test verifying several requirements lists each once; never repeat the same ID.

   The task is not done until the plan's Verification method has actually been run and
   its outcome reported. Waiving it requires asking.
7. **Reconcile the spec.** If implementation forced the behavior to differ from what
   gate 1 approved, the "shall" text changes — that is a **normative** change and it
   **re-opens gate 1**: show the diff, say what forced it, get approval before
   committing. Fixing a wrong cross-reference or clumsy wording is **editorial** and
   needs no approval. **Always report the final spec diff** when you finish, so the
   difference between the two is visible without diffing by hand.
8. **Gate 3 — the pull request.** With the work done, the Verification method run and its
   outcome reported: **stop and ask whether to open a PR.** The user may want a manual
   test run of their own first — that is the point of this gate, so do not pre-empt it.
   Once they confirm, draft the PR title and body and **stop for approval** of that text.
   Give the PR a **human-friendly title** in the same plain-language style as the issue.
   The PR body is where the implementation lives: the why, the requirement IDs, **how the
   issue was resolved** (the approach and structure the issue deliberately omitted), the
   verification actually performed, the coverage number, and `Closes #<issue>` from gate 1b.
   Only then push the branch and `gh pr create`.

Merge to `main` by **squash merge**, so the branch's stage commits — including the spec
commit that briefly ran ahead of its code — never reach `main`.

## Where to look for task X

| Task touches | Read | ID prefix |
|---|---|---|
| PDU/ADU encoding, function codes, exception responses, CRC-16, MBAP header | [`docs/specs/frame/`](./docs/specs/frame/) | `FR-R-*` |
| Async client API, request issuing, response matching, timeouts, retry/reconnect | [`docs/specs/client/`](./docs/specs/client/) | `CL-R-*` |
| Async server, request dispatch, the data store, exception generation | [`docs/specs/server/`](./docs/specs/server/) | `SV-R-*` |
| TCP sockets, RTU serial ports, framing boundaries, connection lifecycle | [`docs/specs/transport/`](./docs/specs/transport/) | `TR-R-*` |
| Platforms, MSRV, performance posture, security, versioning, testing conventions | [`docs/specs/non-functional-requirements.md`](./docs/specs/non-functional-requirements.md) | `NF-R-*` |
| Module graph, data flow, concurrency model | [`ARCHITECTURE.md`](./ARCHITECTURE.md) | — |
| Contribution workflow, conventions | [`CONTRIBUTING.md`](./CONTRIBUTING.md) | — |

Each area's `edge-cases.md` records its **known limitations** — behavior that is
ugly but intentional. Check it before "fixing" something that looks wrong.

## Build / test / lint

```sh
cargo check --all-features
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --check
cargo llvm-cov --all-features --fail-under-lines 80
```

Narrow the loop while iterating — don't run the whole suite for one test:

```sh
cargo test ut_crc                 # one test (unit tests are named ut_*)
cargo test --test tcp_loopback    # one integration test file (it_* functions)
cargo llvm-cov --all-features --html   # browsable per-line coverage report
```

Run these before considering work done — `lefthook` enforces `fmt --check` and
`clippy -D warnings` pre-commit, and CI runs fmt, clippy, check, test, and the
coverage gate as separate steps on every push **and every pull request**, so a
failure is caught either way.

## Conventions

- Unit tests live in `#[cfg(test)] mod tests` at the bottom of the file under test,
  function names prefixed `ut_`. Integration tests live in `tests/`, function names
  prefixed `it_`.
- **Tests bind ephemeral ports only.** Any test that starts a TCP listener binds port 0
  and reads the assigned port back — never a fixed port number. A fixed port makes the
  test fail whenever anything else on the machine holds it (a parallel checkout's tests,
  a stray server). Deliberately *occupying* a port to test bind-failure handling still
  binds the occupier ephemerally first, then points the server at that port.
- **RTU tests do not require real hardware.** Serial behavior is exercised over an
  in-memory or virtual duplex pair; a test that needs a physical `/dev/tty*` is gated
  behind an ignored/feature-flagged test and never runs in CI.
- **Async runtime: Tokio.** The public async surface is runtime-agnostic where it is
  cheap to be, but the implementation and all tests target Tokio. Introducing a second
  runtime abstraction is a scope decision — ask.
- Rust edition 2024, stable toolchain (`rust-toolchain.toml`). The MSRV is a
  non-functional requirement — raising it is a normative change.
- Bare `unwrap` is denied in non-test code; every fallible call documents why it cannot
  fail via `expect("...")`. Tests are exempt.
- **Never split a source file just because it is large.** A split must earn its keep — it
  separates genuinely distinct responsibilities, improves navigability, or cuts coupling. A
  long file that covers one cohesive concern, or is flat generated data (e.g. a function-code
  table), stays whole. Treat a line count as a prompt to *review* the file, not a mandate to
  divide it.
- **Check crates.io before hand-rolling anything.** At the start of every
  implementation stage, list the functionality it needs (byte parsing, checksums, hex,
  serial I/O, async runtime, …) and search crates.io for a popular, maintained crate
  that already provides it. Report downloads, latest release date, and maintenance
  state, and recommend — do not default to writing it yourself. Adding the dependency
  is still a scope boundary below, so the finding goes to the user, not straight into
  `Cargo.toml`.
- **Errors are typed, never stringly.** Failures surface as variants of the crate's error
  enum, not formatted strings a caller has to match on by substring. A new failure mode is
  a new variant — and a new variant is a public API change, so it is spec (gate 1).
- **No panics on wire input.** Malformed, truncated, or hostile bytes from a peer produce
  an error, never a panic, a slice-index panic, or an unbounded allocation. Every decode
  path is written to that standard and tested with truncated input.

## Scope boundaries — check with the user before

- **Adding support for a function code** not already in `docs/specs/frame/api-contract.md`.
  The supported set is a deliberate contract, not an open list.
- **Adding a dependency.** A protocol library's dependency tree is part of its value —
  each addition needs a reason that outweighs it.
- **Changing the public API surface** (renaming a type, altering a signature, adding a
  trait bound). Semver consequences are the user's call.
- **Adding a second async runtime or a sync/blocking API.** The crate is async-first on
  Tokio; a blocking facade is a product decision, not a mechanical addition.
