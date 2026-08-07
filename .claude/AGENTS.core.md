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
- **Every agent's output stays concise and compact** — chat responses, stage/final reports, commit messages, PR and issue bodies, review findings: say the same thing in fewer words whenever fewer words say it. No restating what a diff, file, or prior message already shows. Extra words for the same fact are a defect, not thoroughness — applies to every agent in this workflow, not just the files they write.

## Scope boundaries — ask before

- Supporting a function code not in `docs/specs/frame/api-contract.md`. The supported set is a deliberate contract.
- Adding a dependency.
- Changing the public API surface (renaming a type, altering a signature, adding a trait bound) — semver consequences are the user's call.
- Adding a second async runtime, or a sync/blocking API.
