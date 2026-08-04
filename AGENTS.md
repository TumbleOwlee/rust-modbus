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
- Delegate gates to agents. **All agents run Sonnet or better**, planning and
  implementation alike. Weaker models stop mid-plan and call the rest future work, commit
  a stub as a "green" checkpoint, and report a hanging test as verified.
- **One git worktree per issue, per agent**, at `.claude/worktrees/<slug>` — inside the
  project directory so an agent confined to the project root can reach it, and gitignored
  so it never shows up as untracked content. Two agents in one checkout interleave commits
  and stage each other's work — a branch is not a working tree. Create before the agent
  starts, remove after its branch merges.
- The plan is a contract. An implementer who finds it wrong stops and reports; it does not
  improvise a different design.
- **An agent's report of its own verification is not verification.** Re-run the tools.
- **Every state change moves a card on the task board** below. A run whose state lives only
  in the conversation is lost when the session is.

Verify before every approval request:

- **A plan** — quoted requirement text matches the file, appended IDs are genuinely
  unused, nothing contradicts an existing requirement, and it proposes what was asked.
- **An implementation** — re-run the full build/test/lint/coverage gauntlet yourself in
  the agent's worktree, read the code the report describes, check ID citations sit
  directly below their test attributes, mutation-check any test written after its
  implementation, diff the spec against what was approved.

### Task board

Every gated run keeps its state on disk, so an interrupted session resumes instead of
restarting. The directory a card sits in **is** its state:

```
.claude/tasks/
  open/  inprogress/  inreview/  done/   cards move between these
  artifacts/<slug>/
    spec-diff.md   gate 1 approved normative text
    plan.md        gate 2 stages, steps, dependency tree
    review.md      review findings, keyed by stage id
```

The directories are tracked; the cards inside are gitignored local state. Cards live in
the **main checkout only** — an agent in a worktree is handed the absolute path of its own
card and writes that one file. Never a copy of the board per worktree.

Three kinds of card, all short, all written for agents rather than for reading:

| Card | File | Owner |
|---|---|---|
| parent | `<slug>.md` | orchestrator — one per run |
| stage | `<slug>.s<n>.md` | the implementer working that stage |
| wave gate | `<slug>.w<n>.md` | orchestrator — one per parallel wave |

A card is YAML frontmatter plus an append-only log. Agents **append**; they never rewrite
a log line, so a crash mid-write costs one truncated line instead of the file.

```
---
id: <slug>.s3
parent: <slug>
blocked-by: [<slug>.s2]
files: [src/x.rs, tests/x.rs]
branch: <type>/<slug>-3
worktree: .claude/worktrees/<slug>-3
---
2026-01-02T14:02 spawn agent=impl
2026-01-02T14:05 test-red <ID> rejects_short_input
2026-01-02T14:11 green commit=abc123f
2026-01-02T14:12 gauntlet=pass
```

Parent frontmatter carries `issue`, `branch`, `mode: sequential | parallel(N)`, the `gate1`
and `gate2` approval dates, the current `wave`, and `artifacts`. It never copies the goal
or the normative text — the issue holds the goal, `artifacts/` holds the spec.

What each state means:

| Card | `open` | `inprogress` | `inreview` | `done` |
|---|---|---|---|---|
| stage | created from the approved plan | an agent has taken it | agent claims green | merged into the feature branch and verified by the orchestrator |
| wave gate | wave not started | its stages are running | all its stages done; reviewing the wave's accumulated diff | review clean — the next wave unblocks |
| parent | gate 1 pending | implementing | gate 3 and gate 4 | PR squash-merged |

Rules:

- **No agent writes its own `done`.** An implementer moves its card only as far as
  `inreview`. The orchestrator merges the branch, re-runs the gauntlet, and only then moves
  it to `done`. The board obeys the same rule as everything else here: a self-report is not
  verification.
- **Runnable** means every id in `blocked-by` is in `done/`. Stage `done` means merged into
  the feature branch, which is exactly "the code I depend on is on the branch I branch
  from".
- There is no `blocked/` directory. Blocking is derived from `blocked-by`, so it is stated
  once.
- A card is evidence of intent, never of fact. Git is the fact. See *Resume an interrupted
  run*.

### Gate 1 — spec diff. Stop for approval.

- Propose the normative text itself: the "shall" statements with appended IDs, plus
  `edge-cases.md` entries. Ready to land, not prose about intent.
- Observable design is spec: public type and function signatures, the error enum, feature
  gating.
- Bug fix where the spec is right and the code is wrong: no diff — state the violated
  requirement and continue.
- **Board:** create `open/<slug>.md` and `artifacts/<slug>/` before drafting. On approval,
  write the approved text to `artifacts/<slug>/spec-diff.md` and record `gate1` on the
  parent card. Approved text that exists only in the conversation is the thing this board
  is for.

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

Stages, each broken into numbered file-level steps; a table mapping each new requirement
ID to the test that pins it; a **Verification** section naming the method (unit tests
alone / loopback TCP integration / virtual serial pair / interop against an external
master or slave); expected commits; expected coverage impact.

Plus a **dependency tree**: for every stage, the stages it depends on and the files it
touches. Stages with no path between them and disjoint file sets can run in parallel;
everything else is ordered. Read it as waves — a stage becomes runnable once all its
dependencies are merged. Overlapping file sets are a dependency, not a race to resolve
later. A plan whose stages are all sequential says so explicitly; that is a normal
outcome, not a failure to decompose.

The approval also settles **how it will be implemented**, and the plan is not approved
until it is answered:

- **Sequential** — one implementer agent walks every stage in order.
- **Parallel** — the user gives a maximum number of agents running at once. Waves are
  capped at that number; stages beyond the cap wait for a slot.

Default to sequential when the user expresses no preference. Never infer a concurrency
level from the plan's shape — a dependency tree that permits five parallel stages does
not authorize five agents.

**Board:** on approval write `artifacts/<slug>/plan.md`, record `gate2` and `mode` on the
parent card, move the parent to `inprogress/`, and create one `open/<slug>.s<n>.md` per
stage with its `files` and `blocked-by` copied from the dependency tree. Parallel runs also
get one `open/<slug>.w<n>.md` per wave; sequential runs get a single wave-gate card for the
whole run instead. Stage cards are generated from the plan, so stage ids in the plan and on
the board are the same ids.

### Implement, stage by stage

Sequential: one agent, one worktree, on the feature branch, the stages in plan order. It
moves each stage card `open` → `inprogress` when it starts and → `inreview` when the stage
is green and committed; the orchestrator verifies and moves it to `done`. There is nothing
to merge, so `done` here means committed and verified.

Parallel: one worktree and one branch per agent, `.claude/worktrees/<slug>-<n>` on
`<type>/<slug>-<n>`, branched from the feature branch as it stands when the wave starts.
Never two agents in one worktree. Each wave runs the same cycle:

1. Take the runnable stage cards — `blocked-by` all in `done/` — up to the approved agent
   count. Move the wave-gate card to `inprogress/`.
2. Spawn one implementer per stage, each given its worktree path, its stage only, and the
   absolute path of its own card.
3. Wait for the whole wave. Each agent leaves its card in `inreview/`.
4. Per card: merge the branch into the feature branch, re-run the gauntlet on the merged
   result, then move the card to `done/` and remove the worktree.
5. All stages `done` → wave gate to `inreview/`: an independent reviewer reads the wave's
   accumulated diff on the feature branch. Clean → wave gate to `done/`, which unblocks the
   next wave. Any finding → stop, report, and move the implicated stage cards back to
   `inprogress/` with the finding appended.

A clean wave gate is not an approval stop — gate 2 already approved these stages. A finding,
a red gauntlet or a merge conflict always is.

A merge conflict between two stages of one wave means the dependency tree was wrong — report
it, fix the tree, do not hand-resolve it and continue. An agent that stops mid-wave stops
its wave: the finished branches still merge, the rest is re-planned.

The gates do not change under parallelism. Verification, review and the spec reconcile all
happen once, on the merged feature branch, never per agent.

- TDD order above. A stage is a green checkpoint: compiles, `cargo test`, `cargo clippy
  --all-targets -- -D warnings`, coverage ≥ 80%. Commit every green stage — that is what
  makes the plan resumable.
- Stage messages stay cheap; they are squashed. The squash message carries the requirement
  IDs and the why. The spec is the first stage and the first commit.
- **Never add `Co-Authored-By`, "Generated with", or any other tool attribution trailer**
  to a commit message, PR body, issue or comment. It carries no information about the
  change and it is noise in `git log` forever. This holds for every agent and every gate,
  including the squash message and the gate 4 PR body.
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

### Gate 3 — review. Stop for approval.

Before proposing a PR, run an independent review of the branch in a **separate agent**
that did not write the code — a reviewer sharing the implementer's context reproduces its
blind spots. Give it the diff, the approved spec text, and this file. It reports on:

- **Spec fidelity** — every approved requirement implemented, nothing implemented that was
  not approved (scope creep), no requirement pinned by a test that does not actually
  exercise it.
- **Standards** — the conventions below, test naming and ID citation, error handling.
- **TDD honesty** — tests that could pass against an empty implementation, assertions on
  the implementation's own output, coverage padded by tests that execute without asserting.

Re-run the verification yourself, then report the findings and the fixes. Findings the
user should decide on are raised, not silently fixed.

**Board:** the reviewer appends to `artifacts/<slug>/review.md`, keying each finding to a
stage id so the right cards go back to `inprogress/`. Move the parent card to `inreview/`
when the review starts.

### Gate 4 — pull request. Stop for approval.

- Verification run and reported, then **ask whether to open a PR** — the user may want
  their own manual run first; do not pre-empt it.
- Then draft title and body, get approval of that text, then push and `gh pr create`.
- Title style as the issue. Body is the implementation: the why, the requirement IDs, how
  the issue was resolved (the approach and structure the issue omitted), the verification
  actually performed, the coverage number, `Closes #<issue>`.

### Merge

Squash merge to `main`, so stage commits — including the spec commit that ran ahead of its
code — never reach `main`. Remove the worktree after the merge, and move the parent card to
`done/`. Nothing should remain under `.claude/worktrees/`, and no card for this run should
remain outside `done/`.

### Resume an interrupted run

Cards outside `open/` and `done/` when no agent is running mean a session died mid-run.
Resuming is triggered — by the user, or by `/spec-feature` — never automatic.

**Reconcile before acting.** The card states what an agent intended; git states what
happened, and the agent may have died between the two:

| The card claims | Check | A disagreement means |
|---|---|---|
| a worktree | `git worktree list` | the card is stale |
| a branch | `git rev-parse` | the stage never started |
| `commit=<sha>` | the sha exists and is on that branch | the commit never landed |
| `gauntlet=pass` | re-run it at that sha | the card overstated its state |
| stage `done` | `git branch --contains` against the feature branch | it was never merged, and every stage planned on top of it is planned on a lie |

Report the card-versus-git differences before touching anything. Where they agree, resume.
Where they do not, stop and report: a card lagging behind git is a forgotten move and may
be corrected, but a card claiming work git cannot show is never talked into being true.

After a clean reconcile, resume only what needs no approval — respawn implementers for
already-approved stages, merge finished branches, run wave gates — and halt at the first
gate that needs the user. Recorded `gate1` and `gate2` approvals stay valid; do not re-ask
them.

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
