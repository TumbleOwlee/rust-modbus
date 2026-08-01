# AGENTS.md

Router for AI coding agents working in this repo. Read this first; it points to
everything else.

## What this repo is

`rust-modbus` — a Rust library providing **async Modbus client and server**
capabilities over **RTU** and **TCP**. A single crate, organised into modules; no
binary is shipped. Product framing: [`PRD.md`](./PRD.md). Structure and module map:
[`ARCHITECTURE.md`](./ARCHITECTURE.md).

## Spec-driven — read this before you change behavior

`docs/specs/` is the **authoritative** specification: the code conforms to it, not the
other way around. Read an area's `requirements.md` before editing code in it. A behavior
change with no spec change is incomplete.

**`main` never contains an unfinished spec.** A requirement on `main` is a statement
about code that exists and is tested. A feature branch may hold a spec commit ahead of
its implementation; `main` may not, which the squash merge guarantees.

If the code and the spec already disagree and it is *not* what you were asked to fix:
**stop and raise it as its own task.** Folding it in silently widens approved work, and
the fix deserves its own review.

Specs carry no `file:line` pointers by design — locate code with your own search tools.
Requirements have stable IDs (`FR-R-*`, `CL-R-*`, `SV-R-*`, `TR-R-*`, `NF-R-*`);
reference them in commits and PRs.

## Test-driven — the test comes before the implementation

Within a stage the order is fixed:

1. Write the test that pins the requirement. Its doc comment cites the requirement ID
   (`/// FR-R-012 — …`).
2. **Run it and watch it fail** for the right reason, and report what the failure was. A
   wrong assertion, a compile error in the test itself, or a test that passes before the
   code exists is a test that proves nothing.
3. Write the minimum implementation that makes it pass.
4. Refactor with the test green.

A stage that adds implementation without a preceding failing test is not done, and
neither is a stage whose test was written afterwards to fit the code. Wire protocols are
exactly where after-the-fact tests rot — a test written against the encoder you just
wrote asserts what you built, not what Modbus requires. Where the wire format is
specified by the standard, derive the expected bytes from the spec document, not from a
debug print of your own output.

**Coverage floor: 80% of lines**, gating every push and pull request via `cargo llvm-cov
--fail-under-lines 80`. A floor, not a target: it catches untested modules, it does not
certify tested ones. Never chase it with tests that execute code without asserting.

## Workflow — follow this for every behavior change

This workflow **replaces** any generic workflow skill (including `/workflow`); do not run
one here. `docs/specs/` is already the PRD and the design record — a second
design-artifact system would only give the "why" two homes to diverge in.

**It triggers on behavior change, not on size.** Ask: *does this change what the software
is required to do?* Yes — a new function code, a changed timeout default, a different
error variant, any observable API semantics — the full workflow applies, however small the
diff. No — a refactor, a rename, perf work with identical semantics, tests, docs — there
is no spec diff to approve, so skip the gates and do the work. Size decides how many
*stages* the plan has, never whether the gates exist.

Work on a branch off `main`, never on `main` itself: `<type>/<slug>`, conventional-commit
type (`feat/`, `fix/`, `docs/`).

### Who does which phase

Gates are delegated to agents; the orchestrator verifies between each one.

- **Every agent runs Sonnet** — planning (gates 1, 1b, 2) and implementation alike. Sonnet
  is the floor, not a preference: Haiku was tried and failed three ways — it stopped
  mid-plan and called the remaining stages future work, committed `unimplemented!()` into
  non-test code as a "green" checkpoint, and reported an integration test as verified that
  in fact deadlocked and had never been run.
- **The plan is the contract.** An implementer that finds the plan wrong stops and reports
  rather than improvising a different design.
- **An agent's own report of its verification is not verification.** Every failure above
  was caught by re-running the tools, never by reading the report — which claimed success
  in each case.
- **Verify, then ask** — before the user sees anything. For a plan: every quoted
  requirement text matches the file, appended IDs are genuinely unused, nothing
  contradicts an existing requirement, and what it proposes is what was asked for. For an
  implementation: re-run the whole build/test/lint/coverage gauntlet yourself in the
  agent's worktree, read the code the report describes, check requirement-ID citations sit
  directly below their test attributes, mutation-check any test written after its
  implementation, and diff the spec against what was approved.

**Every agent works in its own git worktree, one per issue.** Two agents in one checkout
interleave their commits, `git add -A` each other's unstaged work into the wrong commit,
and break each other's build — a branch is not a working tree. Create the worktree before
the agent starts; remove it after its branch merges.

1. **Read the affected area's spec** — `requirements.md` *and* `edge-cases.md`, which
   records behavior that is ugly *on purpose*. Routing table below.

2. **Gate 1 — the behavior contract.** Propose the **spec diff itself**: the actual
   "shall" text of new or changed requirements with their appended IDs, plus any
   `edge-cases.md` entries. Not prose about what you intend to build — normative text,
   ready to land. Observable design choices *are* spec and get settled here; for a library
   that includes public type and function signatures, the error enum, and feature-flag
   gating. **Stop for approval.** For a bug fix where the spec is already right and the
   code is wrong there is no diff: state the requirement the code violates and move on.

3. **Gate 1b — the tracking issue.** Once the spec is approved, search open issues
   (`gh issue list`) and closed ones for the **same goal**. If one exists, use it and
   reference its number from here on — never open a second. Otherwise draft title and body,
   **stop for approval**, and run `gh issue create` only once confirmed.

   - **Human-friendly title** — a plain-language summary a maintainer can scan, not a slug,
     a requirement ID, or a restated commit subject.
   - **Self-contained.** The spec still lives only in the working tree, so a reader cannot
     look an ID up: quote the full normative text of every new requirement beside its ID,
     every changed requirement as old → new, and the `api-contract.md` and `edge-cases.md`
     entries. An ID with no text is useless.
   - **Goal and normative changes only.** Never implementation detail — code structure,
     which files or functions change, the chosen approach. That belongs to the plan
     (gate 2) and the PR.
   - **`##` section headers, not a wall of prose**: `## Background` (or `## Why`) for
     problem and context, `## Scope` (or the requirement changes) for what is in scope,
     `## Goal` for the outcome, plus whatever else the issue warrants. Keep enumerations
     compact — grouped ID ranges, not a paragraph per item. Same shape for PR bodies.

4. **Write the spec into the working tree.** Never mark it unfinished in the file — the
   file only ever holds normative text. The plan tracks what is not yet backed by a
   passing test.

5. **Gate 2 — the implementation plan.** Stages, file-level steps, a table mapping each new
   requirement ID to the test that will pin it, and a **Verification** section naming how
   the change will be exercised (unit tests alone; a loopback TCP integration test; a
   virtual serial pair; interop against an external Modbus master/slave). State expected
   commits and expected coverage impact. **Stop for approval.**

6. **Implement, stage by stage — test first**, per the four-step order above. A stage is a
   **green checkpoint**: it compiles, `cargo test` passes, `cargo clippy --all-targets --
   -D warnings` passes, coverage is ≥ 80%. **Commit every green stage** — that is what
   makes the plan resumable after an interrupted session. Stage commits are branch-local
   scaffolding squashed away on merge, so keep their messages cheap; the squash message is
   the one that carries the requirement IDs and the why. The spec is the first stage and so
   the first commit — legal on a branch, never on `main`.

   Every new or changed requirement ships with at least one test citing its ID in a doc
   comment (`/// CL-R-021 — …`). Existing tests are held to the same terms: every test that
   pins observable behavior cites the requirement it verifies. A test of a pure internal or
   helper detail that no requirement governs may stay untagged. A test that verifies real
   behavior no requirement states means the requirement is missing — add it (gate 1) rather
   than attach a loose ID.

   The citing doc comment goes **directly below** the `#[test]`/`#[tokio::test]` attribute,
   immediately above the `fn`. An ID appears **at most once per test** — a test verifying
   several requirements lists each once.

   The task is not done until the plan's Verification method has actually been run and its
   outcome reported. Waiving it requires asking.

7. **Reconcile the spec.** If implementation forced the behavior to differ from what gate 1
   approved, the "shall" text changes — **normative**, so it **re-opens gate 1**: show the
   diff, say what forced it, get approval before committing. A wrong cross-reference or
   clumsy wording is **editorial** and needs no approval. **Always report the final spec
   diff** when you finish, so the difference is visible without diffing by hand.

8. **Gate 3 — the pull request.** With the work done and the Verification method run and
   reported: **stop and ask whether to open a PR.** The user may want a manual test run of
   their own first — that is the point of this gate, so do not pre-empt it. Once they
   confirm, draft the PR title and body and **stop for approval** of that text. Same
   human-friendly title style as the issue. The PR body is where the implementation lives:
   the why, the requirement IDs, **how the issue was resolved** (the approach and structure
   the issue deliberately omitted), the verification actually performed, the coverage
   number, and `Closes #<issue>`. Only then push the branch and `gh pr create`.

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

Each area's `edge-cases.md` records its **known limitations** — behavior that is ugly but
intentional. Check it before "fixing" something that looks wrong.

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

Run these before considering work done. `lefthook` enforces `fmt --check` and `clippy -D
warnings` pre-commit, and CI runs fmt, clippy, check, test and the coverage gate as
separate steps on every push **and every pull request**, so a failure is caught either way.

## Conventions

- Unit tests live in `#[cfg(test)] mod tests` at the bottom of the file under test, named
  `ut_*`. Integration tests live in `tests/`, named `it_*`.
- **Tests bind ephemeral ports only** — port 0, then read the assigned port back. A fixed
  port fails whenever anything else on the machine holds it (a parallel checkout's tests, a
  stray server). Deliberately *occupying* a port to test bind failure still binds the
  occupier ephemerally first, then points the server at that port.
- **RTU tests do not require real hardware.** Serial behavior runs over an in-memory or
  virtual duplex pair; a test needing a physical `/dev/tty*` is ignored or feature-gated and
  never runs in CI.
- **Async runtime: Tokio.** The public async surface is runtime-agnostic where that is
  cheap, but the implementation and all tests target Tokio. A second runtime abstraction is
  a scope decision — ask.
- Rust edition 2024, stable toolchain (`rust-toolchain.toml`). The MSRV is a non-functional
  requirement — raising it is a normative change.
- Bare `unwrap` is denied in non-test code; every fallible call documents why it cannot fail
  via `expect("...")`. Tests are exempt.
- **Never split a source file just because it is large.** A split must earn its keep —
  separating genuinely distinct responsibilities, improving navigability, or cutting
  coupling. A long file covering one cohesive concern, or flat generated data (a
  function-code table), stays whole. A line count is a prompt to *review* a file, not a
  mandate to divide it.
- **Check crates.io before hand-rolling anything.** Each implementation stage starts by
  listing what it needs (byte parsing, checksums, hex, serial I/O, async runtime, …) and
  searching for a popular, maintained crate that already provides it. Report downloads, last
  release date and maintenance state, and recommend — do not default to writing it yourself.
  The dependency is still a scope boundary, so the finding goes to the user, not into
  `Cargo.toml`.
- **Errors are typed, never stringly.** Failures surface as variants of the crate's error
  enum, not strings a caller matches on by substring. A new failure mode is a new variant,
  and a new variant is public API — so it is spec (gate 1).
- **Domain values are typed, never bare integers.** A unit identifier, a data address, a
  quantity, a register value and a transaction identifier are different things that happen
  to share a width; passing one where another is meant shall not compile. Wrap each in its
  own transparent newtype where it enters the crate's API, keeping raw integers only for
  genuinely opaque bytes. A new domain value is public API — so it is spec (gate 1).
- **No panics on wire input.** Malformed, truncated or hostile bytes from a peer produce an
  error, never a panic, a slice-index panic, or an unbounded allocation. Every decode path is
  written to that standard and tested with truncated input.

## Scope boundaries — check with the user before

- **Adding support for a function code** not already in `docs/specs/frame/api-contract.md`.
  The supported set is a deliberate contract, not an open list.
- **Adding a dependency.** A protocol library's dependency tree is part of its value — each
  addition needs a reason that outweighs it.
- **Changing the public API surface** (renaming a type, altering a signature, adding a trait
  bound). Semver consequences are the user's call.
- **Adding a second async runtime or a sync/blocking API.** The crate is async-first on
  Tokio; a blocking facade is a product decision, not a mechanical addition.
