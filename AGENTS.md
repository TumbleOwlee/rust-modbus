# AGENTS.md

Router for AI coding agents. Read first.

## Repo

`rust-modbus` — async Modbus client and server over RTU and TCP. One library crate, no
binary. Product framing: [`PRD.md`](./PRD.md). Structure: [`ARCHITECTURE.md`](./ARCHITECTURE.md).

## Spec-driven

- `docs/specs/` is authoritative. Code conforms to the spec, not the reverse.
- Read the area's `requirements.md` **and** `edge-cases.md` before editing that area.
  `edge-cases.md` records deliberate ugliness — check it before "fixing" something.
- A behavior change with no spec change is incomplete.
- `main` never holds an unfinished spec: a requirement on `main` describes code that
  exists and is tested. A branch may hold a spec commit ahead of its code; the squash
  merge keeps that off `main`.
- Pre-existing spec/code disagreement that is not the task you were given: stop, raise it
  separately. Folding it in widens already-approved work and skips its own review.
- Specs carry no `file:line`. Locate code with search tools.
- Requirement IDs are stable and append-only. Cite them in commits and PRs.

## TDD — fixed order within every stage

1. Write the test. Its doc comment cites the requirement ID (`/// FR-R-012 — …`).
2. Run it, watch it fail for the right reason, report the failure. A wrong assertion, a
   test-side compile error, or a pass before the code exists proves nothing.
3. Minimum implementation that passes.
4. Refactor green.

- Implementation without a preceding failing test: not done. Test written afterwards to
  fit the code: not done.
- Derive expected wire bytes from the Modbus standard, never from a debug print of your
  own encoder.
- Coverage floor 80% of lines, CI-gated on every push and PR. A floor, not a target —
  never inflate it with tests that execute code without asserting.

## Workflow

Triggers on **behavior change, any size**: a new function code, a changed default, a new
error variant, any observable API semantics. Not a behavior change: refactor, rename,
perf work with identical semantics, tests, docs — no gates, just do it. Size sets the
number of stages, never whether the gates exist.

- Replaces any generic workflow skill (`/workflow`); do not run one. `docs/specs/` is
  already the PRD and the design record.
- Branch off `main`, never commit to `main`. `<type>/<slug>`, type ∈ {`feat`, `fix`, `docs`}.
- Delegate gates to agents. **All agents run Sonnet**, planning and implementation alike.
  Sonnet is a floor: Haiku stopped mid-plan and called the rest future work, committed
  `unimplemented!()` as a "green" checkpoint, and reported a deadlocking test as verified.
- **One git worktree per issue, per agent.** Two agents in one checkout interleave commits
  and `git add -A` each other's work — a branch is not a working tree. Create before the
  agent starts, remove after its branch merges.
- The plan is a contract. An implementer who finds it wrong stops and reports; it does not
  improvise a different design.
- **An agent's report of its own verification is not verification.** Re-run the tools.

Verify before every approval request:

- **A plan** — quoted requirement text matches the file, appended IDs are genuinely
  unused, nothing contradicts an existing requirement, and it proposes what was asked.
- **An implementation** — re-run the full build/test/lint/coverage gauntlet yourself in
  the agent's worktree, read the code the report describes, check ID citations sit
  directly below their test attributes, mutation-check any test written after its
  implementation, diff the spec against what was approved.

### Gate 1 — spec diff. Stop for approval.

- Propose the normative text itself: the "shall" statements with appended IDs, plus
  `edge-cases.md` entries. Ready to land, not prose about intent.
- Observable design is spec: public type and function signatures, the error enum, feature
  gating.
- Bug fix where the spec is right and the code is wrong: no diff — state the violated
  requirement and continue.

### Gate 1b — tracking issue. Stop for approval.

- Search `gh issue list` and closed issues for the same goal. Reuse what exists and
  reference its number; never open a second. Otherwise draft, get approval, `gh issue create`.
- Title: plain language a maintainer can scan. Not a slug, an ID, or a commit subject.
- Self-contained — the spec is not pushed yet, so quote the full normative text beside
  each new ID, each changed requirement as old → new, plus `api-contract.md` and
  `edge-cases.md` entries. An ID with no text is useless.
- Goal and normative changes only. No implementation detail (structure, files, functions,
  approach) — that belongs to gate 2 and the PR.
- `##` sections, not prose: `## Background`/`## Why`, `## Scope`, `## Goal`, more as
  warranted. Compact enumerations, grouped ID ranges. Same shape for PR bodies.

### Write the spec into the working tree

Never marked unfinished — the file holds only normative text. The plan tracks what lacks
a passing test.

### Gate 2 — implementation plan. Stop for approval.

Stages; file-level steps; a table mapping each new requirement ID to the test that pins
it; a **Verification** section naming the method (unit tests alone / loopback TCP
integration / virtual serial pair / interop against an external master or slave); expected
commits; expected coverage impact.

### Implement, stage by stage

- TDD order above. A stage is a green checkpoint: compiles, `cargo test`, `cargo clippy
  --all-targets -- -D warnings`, coverage ≥ 80%. Commit every green stage — that is what
  makes the plan resumable.
- Stage messages stay cheap; they are squashed. The squash message carries the requirement
  IDs and the why. The spec is the first stage and the first commit.
- Every new or changed requirement ships ≥ 1 test citing its ID.
- Every existing test that pins observable behavior cites its requirement. Tests of pure
  internal or helper detail may stay untagged. Behavior no requirement states means the
  requirement is missing — add it (gate 1), never attach a loose ID.
- The citation goes directly below `#[test]`/`#[tokio::test]`, immediately above the `fn`.
  Each ID appears at most once per test.
- Not done until the Verification method has been run and its outcome reported. Waiving it
  requires asking.

### Reconcile the spec

Behavior differing from what gate 1 approved is normative and **re-opens gate 1**: show
the diff, state what forced it, get approval before committing. A wrong cross-reference or
clumsy wording is editorial — no approval needed. Always report the final spec diff.

### Gate 3 — pull request. Stop for approval.

- Verification run and reported, then **ask whether to open a PR** — the user may want
  their own manual run first; do not pre-empt it.
- Then draft title and body, get approval of that text, then push and `gh pr create`.
- Title style as the issue. Body is the implementation: the why, the requirement IDs, how
  the issue was resolved (the approach and structure the issue omitted), the verification
  actually performed, the coverage number, `Closes #<issue>`.

### Merge

Squash merge to `main`, so stage commits — including the spec commit that ran ahead of its
code — never reach `main`.

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

## Build / test / lint

```sh
cargo check --all-features
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --check
cargo llvm-cov --all-features --fail-under-lines 80
```

Narrow the loop while iterating:

```sh
cargo test ut_crc                 # one test (unit tests are named ut_*)
cargo test --test tcp_loopback    # one integration test file (it_* functions)
cargo llvm-cov --all-features --html   # browsable per-line coverage report
```

Run the full set before considering work done. `lefthook` enforces `fmt --check` and `clippy -D warnings` pre-commit; CI runs fmt,
clippy, check, test and the coverage gate on every push and pull request.

## Conventions

- Unit tests: `#[cfg(test)] mod tests` at the bottom of the file under test, named `ut_*`.
  Integration tests: `tests/`, named `it_*`.
- Bind port 0 and read the assigned port back; never a fixed port. To test bind failure,
  bind the occupier ephemerally first and point the server at that port.
- No real serial hardware — RTU behavior runs over an in-memory or virtual duplex pair. A
  test needing `/dev/tty*` is ignored or feature-gated and never runs in CI.
- Tokio. The public surface is runtime-agnostic where that is cheap; implementation and
  tests target Tokio. A second runtime is a scope decision.
- Edition 2024, stable toolchain (`rust-toolchain.toml`). MSRV is a non-functional
  requirement — raising it is normative.
- No bare `unwrap` outside tests; `expect("why this cannot fail")`.
- Do not split a file for size alone. A split separates distinct responsibilities,
  improves navigability, or cuts coupling. Cohesive files and flat generated data (a
  function-code table) stay whole.
- Start each implementation stage by listing the functionality it needs (byte parsing,
  checksums, hex, serial I/O, async runtime, …) and searching crates.io. Report downloads,
  last release, maintenance state, and recommend — do not default to hand-rolling. Adding
  the dependency is a scope boundary, so the finding goes to the user, not to `Cargo.toml`.
- Errors are typed, never stringly. A new failure mode is a new enum variant, which is
  public API, which is spec (gate 1).
- Domain values are typed: unit id, data address, quantity, register value and transaction
  id are distinct transparent newtypes wrapped where they enter the API; mixing them must
  not compile. Raw integers only for genuinely opaque bytes. A new domain value is public
  API, which is spec (gate 1).
- No panics on wire input. Malformed, truncated or hostile peer bytes produce a typed
  error — never a panic, a slice-index panic, or an unbounded allocation. Test every
  decode path with truncated input.

## Scope boundaries — ask before

- Supporting a function code not in `docs/specs/frame/api-contract.md`. The supported set
  is a deliberate contract.
- Adding a dependency.
- Changing the public API surface (renaming a type, altering a signature, adding a trait
  bound) — semver consequences are the user's call.
- Adding a second async runtime, or a sync/blocking API.
