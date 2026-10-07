# AGENTS.md

Router for AI coding agents. Read first.

**Concise, compact, facts only.**

## Repo

`rust-modbus` — async Modbus client and server over RTU and TCP. Single library crate, no binary. Product: [`PRD.md`](./PRD.md). Structure: [`ARCHITECTURE.md`](./ARCHITECTURE.md).

## Spec-driven

- `docs/specs/` authoritative. Code conforms to spec, never reverse.
- Before editing an area: pull the headings the task touches from its `requirements.md` and their `edge-cases.md` counterparts (`## Conventions — reading`); the whole file only for a cross-cutting change. `edge-cases.md` = deliberate ugliness; check before "fixing."
- Behavior change with no spec change = incomplete.
- `main` never holds unfinished spec: a requirement on `main` describes code that exists and is tested. A branch may hold a spec commit ahead of its code; squash merge keeps that off `main`.
- Pre-existing spec/code disagreement outside your task: stop, raise separately. Folding it in widens approved work, skips its own review.
- Specs carry no `file:line`. Locate code with search tools.
- Requirement and edge-case IDs (`-R-`, `-E-`) stable, append-only (sole exception: `docs/specs/README.md` rule 2). Cite in commits and PRs.
- Find an entry by `grep -rn <ID or keyword> docs/specs/`; exact file:line: `sh .claude/scripts/extract-id.sh <ID> [<ID> ...]` (batch every ID needed into one call).

## TDD — fixed order, every stage

1. Write the test. Doc comment directly above the declaration cites every requirement or edge-case ID the test pins (`/// FR-R-012 — …`; `docs/specs/README.md` rule 8).
2. Run it, watch it fail for the right reason, report the failure. Wrong assertion / test-side compile error / premature pass proves nothing.
3. Minimum implementation that passes.
4. Refactor green.

- Implementation without a preceding failing test: not done. Test written after the fact to fit code: not done.
- Expected values from the authoritative source (standard/protocol/upstream API) — never a debug print of your own implementation.
- Every new/changed requirement ships ≥1 ID-citing test. Every existing test pinning observable behavior cites its requirement; pure internal/helper-detail tests may stay untagged. Behavior no requirement states = requirement missing — add it (spec change), never attach a loose ID.
- Coverage floor 80% of lines, CI-gated on every push and PR. A floor, not a target — never inflate it with tests that execute code without asserting.

## Workflow

Triggers on **behavior change, any size**: new public function, changed default, new error variant, any observable semantics. Size sets stage count, never gate existence. Non-behavior change (refactor, rename, perf with identical semantics, test-only, docs, tooling): gate 1 skipped, rest runs. Trivial edit (one file, no semantics, no test/CI/build effect — typo, comment, doc wording): no gates, branch + PR.

Gate 1 through gate 4, task board, stage-by-stage implementation, review, PR, merge, and resuming an interrupted run: [`AGENTS.workflow.md`](./AGENTS.workflow.md). Pull one section at a time with `extract-section.sh`.

## Where to look for task X

| Task touches | Read | ID prefix |
|---|---|---|
| PDU structure, function code taxonomy, exception responses, robustness, buffer reuse, serde/Display | [`docs/specs/frame/`](./docs/specs/frame/) | `FR-R-*`, `FR-E-*` |
| Bit/register access, file record access, serial-line diagnostics, MEI | [`docs/specs/frame-data-access/`](./docs/specs/frame-data-access/) | `FR-R-*` (new: `FR-DA-R-*`), `FR-DA-E-*` |
| RTU/TCP/ASCII ADU, RTU over byte stream, CRC-16, MBAP header, framing abstraction | [`docs/specs/frame-adu/`](./docs/specs/frame-adu/) | `FR-R-*` (new: `FR-ADU-R-*`), `FR-ADU-E-*` |
| Async client API, request issuing, response matching, timeouts, retry/reconnect | [`docs/specs/client/`](./docs/specs/client/) | `CL-R-*`, `CL-E-*` |
| Async server, request dispatch, the data store, exception generation | [`docs/specs/server/`](./docs/specs/server/) | `SV-R-*`, `SV-E-*` |
| TCP sockets, RTU serial ports, framing boundaries, connection lifecycle | [`docs/specs/transport/`](./docs/specs/transport/) | `TR-R-*`, `TR-E-*` |
| Platforms, toolchain, performance posture, security, versioning, testing conventions | [`docs/specs/non-functional-requirements.md`](./docs/specs/non-functional-requirements.md) | `NF-R-*` |
| Module graph, data flow, concurrency model | [`ARCHITECTURE.md`](./ARCHITECTURE.md) | — |
| Contribution workflow, conventions | [`CONTRIBUTING.md`](./CONTRIBUTING.md) | — |

## Build / test / lint

```sh
cargo fmt --check
cargo clippy --all-features --all-targets -- -D warnings
cargo check --all-features
cargo test --all-features
cargo llvm-cov --all-features --fail-under-lines 80
```

Narrow the loop while iterating:

```sh
cargo test ut_crc                 # one unit test
cargo test --test tcp_loopback    # one integration test file
cargo llvm-cov --all-features --html   # browsable per-line coverage
```

Full set before done. `lefthook` enforces fast checks pre-commit; CI runs the full set on every push and PR.

## RTK

<!-- rtk-instructions v2 -->
### Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

### RTK Commands by Workflow

#### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
rtk tsc                 # TypeScript errors grouped by file/code (83%)
rtk lint                # ESLint/Biome violations grouped (84%)
rtk prettier --check    # Files needing format only (70%)
rtk next build          # Next.js build with route metrics (87%)
```

#### Test (60-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
rtk go test             # Go test failures only (90%)
rtk jest                # Jest failures only (99.5%)
rtk vitest              # Vitest failures only (99.5%)
rtk playwright test     # Playwright failures only (94%)
rtk pytest              # Python test failures only (90%)
rtk rake test           # Ruby test failures only (90%)
rtk rspec               # RSpec test failures only (60%)
rtk test <cmd>          # Generic test wrapper - failures only
```

#### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git add             # Ultra-compact confirmations (59%)
rtk git commit          # Ultra-compact confirmations (59%)
rtk git push            # Ultra-compact confirmations
rtk git pull            # Ultra-compact confirmations
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
rtk git worktree        # Compact worktree
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

#### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

#### JavaScript/TypeScript Tooling (70-90% savings)
```bash
rtk pnpm list           # Compact dependency tree (70%)
rtk pnpm outdated       # Compact outdated packages (80%)
rtk pnpm install        # Compact install output (90%)
rtk npm run <script>    # Compact npm script output
rtk npx <cmd>           # Compact npx command output
rtk prisma              # Prisma without ASCII art (88%)
rtk uv run <cmd>        # Compact uv project command output
```

#### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%). Format flags (-c, -l, -L, -o, -Z) run raw.
rtk find <pattern>      # Find grouped by directory (70%)
```

#### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

#### Infrastructure (85% savings)
```bash
rtk docker ps           # Compact container list
rtk docker images       # Compact image list
rtk docker logs <c>     # Deduplicated logs
rtk kubectl get         # Compact resource list
rtk kubectl logs        # Deduplicated pod logs
```

#### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

#### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
rtk init --global       # Add RTK to ~/.claude/CLAUDE.md
```

### Token Savings Overview

| Category | Commands | Typical Savings |
|----------|----------|-----------------|
| Tests | vitest, playwright, cargo test | 90-99% |
| Build | next, tsc, lint, prettier | 70-87% |
| Git | status, log, diff, add, commit | 59-80% |
| GitHub | gh pr, gh run, gh issue | 26-87% |
| Package Managers | pnpm, npm, npx | 70-90% |
| Files | ls, read, grep, find | 60-75% |
| Infrastructure | docker, kubectl | 85% |
| Network | curl, wget | 65-70% |

Overall average: **60-90% token reduction** on common development operations.
<!-- /rtk-instructions -->

## Conventions — reading

- **Never read a whole file when only part is needed.** Any `.md` (specs, `SKILL.md`s, other repos' docs): `sh .claude/scripts/extract-section.sh '<heading>' ['<heading>' ...] <file>` (unknown heading: `sh .claude/scripts/list-sections.sh <file>` first). Other large files: `sed -n '<start>,<end>p' <file>`. **Enforced:** a `PreToolUse` hook (`.claude/scripts/hook-guard-shell.sh`) denies an unpiped `cat` of a markdown or large file, an unscoped `git show`/`git diff`, an unscoped `find -type f/d`, a raw `gh issue view`, and a raw `gh pr view`. A denial = convention about to be bypassed; follow the redirect, don't retry the command differently.
- **Filter shell output before it lands in context.** `find -name`/`-path`, `git show --stat` or a path filter before full content, `grep`/`tail -N`/`head -N` on test and coverage output.
- **Don't re-run a read-only command whose output is already in context** (`git diff`, `git log`, `git show` on the same refs/paths). Scroll back.
- Read an existing issue with `bash .claude/scripts/issue-view.sh <number|key>` (filesystem tracker: `.claude/issues/<slug>.md`) and an existing PR's body/comments with `bash .claude/scripts/pr-view.sh <number>`, never raw `gh issue view`/`gh pr view` — both hit the GitHub Projects-Classic GraphQL bug (`projectCards`), with or without `--comments`. Inline review threads go through `bash .claude/scripts/pr-feedback.sh fetch|reply` (`AGENTS.workflow.md` *PR feedback*) — into a file, one status line in context.

## Conventions — code

- Unit tests: `#[cfg(test)] mod tests` at bottom of file under test, functions named `ut_*`.
- Integration tests: `tests/`, functions named `it_*`.
- Bind port 0, read the assigned port back — never a fixed port. To test bind failure, bind the occupier ephemerally first, point the server at that port.
- No real serial hardware — RTU behavior runs over an in-memory or virtual duplex pair. A test needing `/dev/tty*` is ignored or feature-gated and never runs in CI.
- **Never split a source file just because it is large.** A split must separate distinct responsibilities, improve navigability, or cut coupling. One cohesive concern or flat generated data stays whole.
- Start each stage by listing needed functionality and searching crates.io. Report downloads, last release, maintenance state, recommend — don't default to hand-rolling. Adding a dependency is a scope boundary: the finding goes to the user, not the manifest.
- Errors typed, never stringly. New failure mode = new error variant = public API = spec (gate 1).
- Domain values typed: unit id, data address, quantity, register value, and transaction id are distinct transparent newtypes wrapped at API entry; mixing them must not compile. Raw integers only for genuinely opaque bytes. New domain type = public API = spec (gate 1).
- **Model states as variants (enum / sum type / tagged union), never a flag plus dependent optionals.** A record whose fields are meaningful only under some combination of its own booleans pushes validation into a resolve step and lets the wire carry states the code must then reject. Give each state its own variant holding exactly the fields that state needs, so invalid combinations cannot be constructed or deserialized at all. A check that genuinely can't be a type (a non-empty list) stays one condition on one variant, never a rule spanning fields. Applies to config and file schemas as much as in-memory state.
- No panics on wire input. Malformed, truncated, or hostile peer bytes produce a typed error — never a panic, a slice-index panic, or an unbounded allocation. Test every decode path with truncated input.
- **A comment says what the code cannot.** No restating the adjacent statement/field/function name; no step narration (`// Create app state`); no banners or import-group headers; no paragraph where a sentence does. **Never cite this workflow** — plan, stage id (`s7`), gate (`Gate3#2`), task item, `(Shared)`, "sanctioned change", "review fix", "manual-exercise fix": rots when the plan is deleted, meaningless to a later reader. Keep the technical content, drop the citation. Requirement IDs are the only sanctioned cross-reference. A lint-suppression justification names the condition that lifts it, never the stage that will. Applies to line and doc comments alike.
- Edition 2024, stable toolchain (`rust-toolchain.toml`); MSRV bump is normative (non-functional requirement).
- No bare `unwrap` outside tests; `expect("why this cannot fail")`.

## Conventions — text

- Specs and AI-facing files (skills, agents, `AGENTS.md`/`CLAUDE.md` itself) stay concise and compact: facts only, no prose, no filler, zero information loss. Every word an agent must re-read on every load; padding is recurring cost, not one-time.
- **Every agent's output stays concise and compact** — chat responses, status lines, card logs, commit messages, PR and issue bodies, review findings: say the same thing in fewer words whenever fewer words say it. No restating what a diff, file, or prior message already shows.
- **No hard line wrap on anything posted externally** — issue bodies, PR bodies, PR/review comments. The host (GitHub, Jira, …) soft-wraps for display; a manually inserted `\n` mid-sentence survives rendering as a real line break. Paragraphs as single unbroken lines; only headings, list items, and code blocks get their own line.
- **Spec text never wrapped** — `docs/specs/` and any `spec-diff.md`: one requirement or edge-case entry, one physical line, however long, so `grep` returns the whole entry and `extract-id.sh` points at one line. **Commit messages are the exception: wrap** — subject ≤ 72 columns, body at 72. `git log` never soft-wraps.
- **No tool attribution trailers** — no `Co-Authored-By` for an assistant, no "Generated with" line — in commits, PR bodies, issues, comments. **Enforced:** `.claude/scripts/hook-guard-attribution.sh` denies the command, `--body-file` content included.

## Scope boundaries — ask before

- Supporting a function code not in `docs/specs/frame/api-contract.md`. The supported set is a deliberate contract.
- Adding a dependency.
- Changing the public API surface (renaming a type, altering a signature, adding a trait bound) — semver consequences are the user's call.
- Adding a second async runtime, or a sync/blocking API.
