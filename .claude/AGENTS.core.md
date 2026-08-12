Excerpt of AGENTS.md — spec-driven core, TDD order, build/test/lint, conventions, scope boundaries. Full gates and task board: ../AGENTS.md. Regenerate by re-copying these sections if they change.

## Spec-driven

- `docs/specs/` authoritative. Code conforms to spec, never reverse.
- Read area's `requirements.md` + `edge-cases.md` before editing that area. `edge-cases.md` = deliberate ugliness; check before "fixing."
- Behavior change with no spec change = incomplete.
- `main` never holds unfinished spec: a requirement on `main` describes code that exists and is tested. A branch may hold a spec commit ahead of its code; squash merge keeps that off `main`.
- Pre-existing spec/code disagreement outside your task: stop, raise separately. Folding it in widens approved work, skips its own review.
- Specs carry no `file:line`. Locate code with search tools.
- Requirement IDs stable, append-only. Cite in commits and PRs.
- One requirement, one physical line, never wrapped — find any by `grep -rn <ID or keyword> docs/specs/`, or with the exact file:line to edit: `sh .claude/scripts/extract-id.sh <ID> [<ID> ...]` (batch every ID needed into one call). Read one section of a large spec file instead of the whole thing: `sh .claude/scripts/extract-section.sh '## <heading>' path/to/file.md`.

## TDD — fixed order, every stage

1. Write the test. Doc comment cites requirement ID (`/// FR-R-012 — …`).
2. Run it, watch it fail for the right reason, report the failure. Wrong assertion / test-side compile error / premature pass proves nothing.
3. Minimum implementation that passes.
4. Refactor green.

- Implementation without a preceding failing test: not done. Test written after the fact to fit code: not done.
- Expected values from the authoritative source (Modbus standard) — never a debug print of your own implementation. Coverage floor 80% of lines, CI-gated on every push and PR — never inflate it with tests that execute code without asserting.

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

## Conventions

- Unit tests: `#[cfg(test)] mod tests` at bottom of file under test, functions named `ut_*`.
- Integration tests: `tests/`, functions named `it_*`.
- Bind port 0, read the assigned port back — never a fixed port. To test bind failure, bind the occupier ephemerally first, point the server at that port.
- No real serial hardware — RTU behavior runs over an in-memory or virtual duplex pair. A test needing `/dev/tty*` is ignored or feature-gated and never runs in CI.
- Don't split a file for size alone. Split for distinct responsibilities, navigability, or coupling. Cohesive files and flat generated data stay whole.
- Start each stage by listing needed functionality and searching crates.io. Report downloads, last release, maintenance state, recommend — don't default to hand-rolling. Adding a dependency is a scope boundary: the finding goes to the user, not the manifest.
- Errors typed, never stringly. New failure mode = new error variant = public API = spec (gate 1).
- Domain values typed: unit id, data address, quantity, register value, and transaction id are distinct transparent newtypes wrapped at API entry; mixing them must not compile. Raw integers only for genuinely opaque bytes. New domain type = public API = spec (gate 1).
- No panics on wire input. Malformed, truncated, or hostile peer bytes produce a typed error — never a panic, a slice-index panic, or an unbounded allocation. Test every decode path with truncated input.
- Edition 2024, stable toolchain (`rust-toolchain.toml`); MSRV bump is normative (non-functional requirement).
- No bare `unwrap` outside tests; `expect("why this cannot fail")`.
- Specs and AI-facing files (skills, agents, `AGENTS.md`/`CLAUDE.md` itself) stay concise and compact: facts only, no prose, no filler, zero information loss. Every word an agent must re-read on every load; padding is recurring cost, not one-time.
- Never read a whole file when only part is needed, filter shell output before it lands in context — not after: `sh .claude/scripts/extract-section.sh` (see `list-sections.sh` first if the heading's unknown) or `sed -n '<start>,<end>p'` instead of `cat`/Read on a whole file; narrow `find`/`git show`/`git diff` at the shell instead of dumping the full output. Applies equally to the Read tool and a Bash `cat` — both cost the same context. Don't re-run a read-only command for output already in context this session — `git diff`, `git log`, `git show`, etc. against the same refs/paths already shown once — scroll back instead of re-running. **Enforced, not just advisory:** a `PreToolUse` hook (`.claude/scripts/hook-guard-shell.sh`) denies an unpiped `cat` of a markdown or large file, an unscoped `git show`/`git diff`, an unscoped `find -type f/d`, a raw `gh issue view`, and a raw `gh pr view`. A denial here means take the message's redirect, not retry the same command differently.
- Read an existing PR's body/comments with `sh .claude/scripts/pr-view.sh <number>`, never raw `gh pr view` — sidesteps a GitHub Projects-Classic GraphQL bug (`repository.pullRequest.projectCards`) that errors on the raw command, with or without `--comments`.
- **Every agent's output stays concise and compact** — chat responses, stage/final reports, commit messages, PR and issue bodies, review findings: say the same thing in fewer words whenever fewer words say it. No restating what a diff, file, or prior message already shows. Extra words for the same fact are a defect, not thoroughness — applies to every agent in this workflow, not just the files they write.

## Scope boundaries — ask before

- Supporting a function code not in `docs/specs/frame/api-contract.md`. The supported set is a deliberate contract.
- Adding a dependency.
- Changing the public API surface (renaming a type, altering a signature, adding a trait bound) — semver consequences are the user's call.
- Adding a second async runtime, or a sync/blocking API.
