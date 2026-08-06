---
name: project-init
description: Interactive one-shot bootstrap of a forked repo-template into a real project — asks for project name, purpose, language stack, capability areas and coverage floor, then writes AGENTS.md, CLAUDE.md, PRD.md, ARCHITECTURE.md, CONTRIBUTING.md, docs/specs/, CI and git hooks, and removes the template scaffolding. Use when the user runs /init, /project-init, says "initialize this repo", "set up the project", or the repo still contains templates/ and a bootstrap CLAUDE.md.
---

# Project init

**Concise, compact, facts only.**

Bootstrap forked template into a configured spec-driven TDD project. One run; then template scaffolding is gone and repo looks like a normal project.

Source of truth: `templates/`. Read the template files — do not reproduce from memory, do not paraphrase workflow gate text (gate text is the product).

## Guard rails

- Ask, never guess: every unknown below is an `AskUserQuestion`, not an assumption. A wrong build command baked into AGENTS.md is worse than one question.
- Never fabricate requirements: `docs/specs/*/requirements.md` ships as an empty, header-only stub. Real "shall" statements are written later through gate 1. Same for ungrounded PRD sections — `*(TBD)*`.
- Never overwrite a file the user wants kept — step 1 settles that per file.
- Do not start implementing the product. This skill sets up the workflow; the first feature goes through gate 1 afterwards.
- Secrets (Jira API token, Bitbucket app password) collected via `AskUserQuestion` like everything else, written once to their `.claude/*.local.json` file, never echoed back, never left ungitignored.

## 0. Check the toolchain

Check whether `caveman` skill is available (listed as `caveman` or `caveman:caveman`). If missing, tell the user they can install it and give the command to run themselves (not something you run for them):

```sh
claude plugin marketplace add JuliusBrussee/caveman && claude plugin install caveman@caveman
```

Optional — don't block on it. Continue to step 1 either way.

## 1. Detect state

Check what exists: `AGENTS.md`, `CLAUDE.md`, `.github/copilot-instructions.md`, `PRD.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `README.md`, `docs/specs/`, `.github/workflows/`, `bitbucket-pipelines.yml`, `.lefthook.yml`, any language manifest (`Cargo.toml`, `package.json`, `pyproject.toml`, `setup.py`, `go.mod`, `CMakeLists.txt`).

Pristine fork = only bootstrap `CLAUDE.md`, `README.md`, `templates/`, `.claude/` — treat these as template scaffolding to replace, not user content; don't ask about them.

`.claude/tasks/` (agent task board) and `.claude/settings.json` (its `SessionStart` detector) ship ready-to-use: create nothing there, ask nothing about them, delete nothing — an empty board is correct pre-first-feature state.

Any other pre-existing target file: show it, ask keep / overwrite / merge. Record the decision — step 7 honours it.

If `AGENTS.md` already exists and looks like this workflow (contains "Gate 1"): say so, ask whether user wants a re-run (re-ask everything, rewrite) or a targeted edit. A re-run on a live project silently discards local customisation — make that explicit before proceeding.

**PR host.** Read the remote: `git remote get-url origin` (fall back to `git config --get remote.origin.url`). Determines `{{PR_OPEN_LINE}}` (step 7). No remote → `{{PR_OPEN_LINE}}` = manual variant below, nothing to detect.
- `github.com` → `{{PR_OPEN_LINE}}` = `` Open via `gh pr create`. `` — no action here; `gh` already covers it.
- `bitbucket.org` → Bitbucket-hosted, handled in step 4c below (sets `{{PR_OPEN_LINE}}`).
- anything else (GitLab, self-hosted, unrecognized) → `AskUserQuestion`: **set up instructions** or **skip**. Either way `{{PR_OPEN_LINE}}` = manual variant below — this skill doesn't know the host's auth/CLI shape well enough to template automation for it. Set up → additionally give the host's CLI/API setup pointer generically (its hosted CLI if one exists, else its REST API + a personal access token) in chat, not in any generated file. Skip → note in the final report that PR opening (gate 4) is manual for this host.

Manual variant (no host automation): `{{PR_OPEN_LINE}}` = `No supported host automation configured — report the drafted title/body and ask the user to open the PR themselves.`

## 2. Ask the project facts

One `AskUserQuestion` round, batching what you can't infer. Infer first from git remote name, directory name, existing README/manifest — then ask to confirm rather than ask blind.

- **Project name** — default: repo directory name.
- **One-line description** — for PRD overview and manifest.
- **Kind** — library / binary / service / CLI / TUI / other. Drives whether ARCHITECTURE.md discusses a public API surface or a process lifecycle.
- **License** — only if `LICENSE` absent, only to decide whether to mention one; don't write a license file unless asked.

## 3. Ask the stack

Detect from manifests if present, else ask. Supported stacks, one file each under `templates/stacks/`:

| Stack | File | Manifest marker |
|---|---|---|
| Rust | `templates/stacks/rust.md` | `Cargo.toml` |
| Python | `templates/stacks/python.md` | `pyproject.toml`, `setup.py` |
| Node / TypeScript | `templates/stacks/node.md` | `package.json` |
| Go | `templates/stacks/go.md` | `go.mod` |
| C / C++ (CMake) | `templates/stacks/cmake.md` | `CMakeLists.txt` |

Read the chosen stack file — sole source for that stack's build/test/lint commands, narrow-loop commands, test naming conventions, coverage tool, CI job matrix, pre-commit hook bodies. Fills every `{{…}}` slot the other templates carry.

**Confirm the command block with the user before writing anywhere.** Show exact commands, let them correct any line. If the stack file offers variants (pnpm vs npm, ninja vs make, ruff vs flake8), ask which — don't pick silently.

Stack not in the table: ask for the five commands (build/check, test, lint, format-check, coverage) and unit/integration test naming convention, then proceed with those; skip stack-specific CI/hook fragments; generate a minimal CI job running exactly those commands.

## 4. Ask the capability areas

Areas are the spine of `docs/specs/` and AGENTS.md's routing table. Ask for 2–6, grounded in what you know about the project — give concrete example areas for *this* project, not an abstract prompt.

Per area, agree:

- a directory name (lowercase, short: `frame`, `client`, `transport`),
- a one-line "covers" description for the routing tables,
- a **requirement ID prefix**: two letters + `-R-` (`FR-R-nnn`, `CL-R-nnn`). Must be unique, must not collide with `NF-R-*` (reserved for non-functional requirements).

If user doesn't know the areas yet: don't invent them — write `docs/specs/README.md` with the "populate as you go" note, create only `non-functional-requirements.md`, put a TBD row in the AGENTS.md routing table. Workflow still functions — gate 1 creates the first area.

Also ask, same round:

- **Coverage floor** (default 80, CI-gated). "None" allowed — removes the coverage line from AGENTS.md, CONTRIBUTING.md and CI.
- **Per-area starting files** — default `requirements.md` + `edge-cases.md`; add `api-contract.md` for anything with a public surface, `data-contract.md` for anything with a wire or file format.

## 4b. Issue tracker

Separate `AskUserQuestion`, options **GitHub / Jira / Filesystem / None** — each sets `{{ISSUE_WORKFLOW}}` from a different `templates/fragments/` file, copy its body verbatim into AGENTS.md's gate 1b section, no paraphrasing.

- **GitHub** → `templates/fragments/issue-github.md`. Then check `gh auth status`. Authenticated → continue silently. Not authenticated / `gh` missing → tell the user, give the exact fix (`gh auth login`, or install from [cli.github.com](https://cli.github.com)); don't block the rest of setup on it, gate 1b will fail loudly on its own later if unfixed.
- **Jira** → second `AskUserQuestion`: **MCP server** or **API credentials**. - MCP → `templates/fragments/issue-jira-mcp.md`. Ask the user to confirm a Jira MCP server is already configured (`claude mcp list`); if not, tell them to add one before running gate 1b — don't block bootstrap on it, no standard install command to fall back on. - Credentials → `templates/fragments/issue-jira-credentials.md`. Collect **Jira base URL**, **email**, **API token** via `AskUserQuestion`, one call, up to 3 questions batched — each with a best-guess option (base URL from the org/repo name, email from git config) so confirming is one click and the exact value always goes through `Other`. Write the answers verbatim to `.claude/jira.local.json`:
    ```json
    { "baseUrl": "https://your-domain.atlassian.net", "email": "you@example.com", "apiToken": "..." }
    ```
    Never echo the token back in chat once written. `.gitignore` already carries `.claude/jira.local.json` unconditionally (ships in the template itself) — nothing more to do here.
- **Filesystem** → `templates/fragments/issue-filesystem.md`. No credentials, no external check. Ensure `.claude/issues/.gitkeep` gets created in step 7 so the empty directory is tracked.
- **None** → `templates/fragments/issue-none.md`. `{{CLOSES_CLAUSE}}` empty.

`{{CLOSES_CLAUSE}}` per choice: GitHub → `` , `Closes #<issue>` `` · Jira → `, references <ISSUE-KEY>` · Filesystem → empty (PR body already carries the goal) · None → empty.

## 4c. Bitbucket credentials

Only runs if step 1 detected `bitbucket.org` as the remote host — independent of the step 4b tracker choice (Bitbucket here is the *PR host*, gate 4; a project can still track issues in Jira or nowhere).

`AskUserQuestion`: **set up Bitbucket credentials now** or **skip**.

Skip → `{{PR_OPEN_LINE}}` = the manual variant (step 1). No file written.

Set up → collect via `AskUserQuestion`, one call, batched, each with a best-guess option pre-filled from the remote URL / git config, exact value via `Other`:

- **Workspace** — the segment right after `bitbucket.org/` in the remote URL.
- **Repo slug** — the segment after the workspace.
- **Username** or **email** (Bitbucket accepts either for app-password auth).
- **App password** — [Bitbucket App passwords](https://bitbucket.org/account/settings/app-passwords/), `Repositories: Write` + `Pull requests: Write` scopes minimum.

Write verbatim to `.claude/bitbucket.local.json`:
```json
{ "workspace": "...", "repoSlug": "...", "username": "...", "appPassword": "..." }
```
Never echo the app password back in chat once written. `.gitignore` already carries `.claude/bitbucket.local.json` unconditionally — nothing more to do here. Sets `{{PR_OPEN_LINE}}` = `` Open via the Bitbucket REST API (`/2.0/repositories/<workspace>/<repo>/pullrequests`), using `.claude/bitbucket.local.json`. `` — substitute the real `<workspace>`/`<repo>` values, not the literal placeholders.

## 5. Ask the scope boundaries

AGENTS.md ends with "Scope boundaries — ask before". Generic entries are dead weight; project-specific ones are the most valuable lines in the file. Propose 3–5 drawn from what you know (adding a dependency, changing the public API surface, supporting a new protocol version, adding a second runtime); let the user edit, drop or add. Keep only entries true for this project.

## 6. Confirm the plan

Before writing anything, show a compact summary: project name, stack, command block, areas with prefixes, coverage floor, tracker, files to be created / overwritten / deleted. Get a yes.

This is the only approval gate in this skill. After it, write everything without further prompting.

## 7. Write the files

`.tmpl` suffix = has `{{PLACEHOLDER}}`s, needs substitution. No suffix = static, copy byte-for-byte, no read-and-rewrite. Check which before touching a file — writing a static file through the substitution pass is harmless but wasted work; skipping substitution on a `.tmpl` ships a literal `{{PLACEHOLDER}}`.

For `.tmpl` files: read, substitute every placeholder, write to the repo root (drop the `.tmpl` suffix). Never leave a `{{PLACEHOLDER}}` unfilled — that's a bug. Grep written files for `{{` before reporting; a leak means a missed slot.

Four things substitution alone doesn't handle:

- **`<!-- PRUNE ME -->` blocks.** `AGENTS.md.tmpl` marks its stack-neutral conventions this way. Drop bullets that don't apply to this project, merge any the stack block restates in stack-specific terms (keep the specific wording), delete the marker comment. Two bullets saying the same thing in different words is how a conventions section starts getting ignored.
- **Source paths in the hook blocks.** The `spec-reminder` and `test-id-reminder` hooks match product code by path (`^src/`, `^(src|include)/`, `*.go`). Point them at this project's actual source directory, or the reminder never fires.
- **No line-wrapping, anywhere.** One logical statement/paragraph stays one physical line, however long a substituted slot makes it (coverage line, stack name, area description) — never hard-wrapped to a column count. Keeps every line `grep -n`-able for its full text, not just a truncated first fragment.
- **`.claude/AGENTS.core.md`.** After `AGENTS.md` is fully substituted and pruned, extract the spans marked `<!-- CORE:BEGIN … --> … <!-- CORE:END … -->` (Spec-driven, Build/test/lint, Conventions, Scope boundaries) into a new file `.claude/AGENTS.core.md`, in that order, under a one-line header: `Excerpt of AGENTS.md — spec-driven core, build/test/lint, conventions, scope boundaries. Full gates and task board: ../AGENTS.md. Regenerate by re-copying these sections if they change.` Then strip the `CORE:BEGIN`/`CORE:END` marker comments from the copy that becomes `AGENTS.md` itself — they're authoring metadata, not part of the router. This is what lets a spawned `spec-planner`/`spec-implementer`/`spec-reviewer` read one short file instead of the whole router.

| Template | Output |
|---|---|
| `templates/AGENTS.md.tmpl` | `AGENTS.md`, plus derived `.claude/AGENTS.core.md` (see above) |
| `templates/CLAUDE.md` | `CLAUDE.md` (static — copy as-is, replaces the bootstrap one) |
| `templates/.github/copilot-instructions.md` | `.github/copilot-instructions.md` (static — copy as-is, same router pattern for GitHub Copilot) |
| `templates/PRD.md.tmpl` | `PRD.md` |
| `templates/ARCHITECTURE.md.tmpl` | `ARCHITECTURE.md` |
| `templates/CONTRIBUTING.md.tmpl` | `CONTRIBUTING.md` |
| `templates/README.md.tmpl` | `README.md` (replaces the template's own) |
| `templates/docs/specs/README.md.tmpl` | `docs/specs/README.md` |
| `templates/docs/specs/non-functional-requirements.md` | `docs/specs/non-functional-requirements.md` (static — copy as-is) |
| `templates/docs/specs/area/*.tmpl` | `docs/specs/<area>/*` — once per area, per step 4 |
| `templates/.claude/scripts/extract-section.sh` | `.claude/scripts/extract-section.sh` (static — copy as-is, `chmod +x`) |
| stack file's `ci` block | `.github/workflows/check.yml` — **only if step 1's remote is `github.com`** |
| stack file's `bitbucket-pipelines` block | `bitbucket-pipelines.yml` — **only if step 1's remote is `bitbucket.org`** |
| stack file's `lefthook` block | `.lefthook.yml` — always, host-agnostic |
| stack file's `config` blocks | stack config files (e.g. `clippy.toml`, `ruff.toml`) — only those the stack file marks as default |

Other host or no remote (step 1) → write neither CI file; note in the final report (step 9) that CI is unset up and must be added by hand for this host.

Also append the stack's build artifacts to `.gitignore` (`target/` for Rust, `node_modules/` and `dist/` for Node, `.venv/`, `__pycache__/`, `.pytest_cache/`, `.mypy_cache/` for Python, `cover.out` for Go, `build/` for CMake). The template `.gitignore` carries only language-agnostic entries and a comment saying so — replace that comment with the real entries.

**Jira credentials chosen (step 4b):** write `.claude/jira.local.json` with the three values collected, exact content, no placeholders left in it — this file holds a live secret, never goes through the `.tmpl` substitution pass.

**Filesystem tracker chosen:** create `.claude/issues/.gitkeep` so the empty directory survives the initial commit.

**Bitbucket credentials chosen (step 4c):** write `.claude/bitbucket.local.json` with the four values collected, exact content, no placeholders left in it — live secret, never goes through the `.tmpl` substitution pass.

Placeholders used across templates:

| Placeholder | From |
|---|---|
| `{{PROJECT_NAME}}`, `{{ONE_LINER}}`, `{{PROJECT_KIND}}` | step 2 |
| `{{STACK_NAME}}`, `{{FULL_COMMANDS}}`, `{{NARROW_COMMANDS}}`, `{{UNIT_TEST_CONVENTION}}`, `{{INTEGRATION_TEST_CONVENTION}}`, `{{ID_CITATION_EXAMPLE}}`, `{{STACK_CONVENTIONS}}`, `{{SETUP_STEPS}}` | step 3 stack file |
| `{{AREA_ROUTING_TABLE}}` | step 4 — AGENTS.md rows, links relative to repo root (`./docs/specs/<area>/`) |
| `{{AREA_TABLE}}` | step 4 — **link base differs per file**: `./<area>/` in `docs/specs/README.md`, `./docs/specs/<area>/` in `PRD.md`. Same rows, different hrefs; get this wrong and every link in one of the two files is dead. |
| `{{COVERAGE_FLOOR}}`, `{{COVERAGE_LINE}}` | step 4 |
| `{{ISSUE_WORKFLOW}}` | step 4b tracker choice |
| `{{PR_OPEN_LINE}}` | step 1 remote detection + step 4c (Bitbucket) |
| `{{SCOPE_BOUNDARIES}}` | step 5 |
| `{{AREA_TITLE}}`, `{{AREA_COVERS}}`, `{{AREA_PREFIX}}` | step 4, per area file |
| `{{ID_CITATION_BLOCK}}`, `{{COVERAGE_CONTRIB_LINE}}` | stack file + coverage floor |
| `{{ID_PREFIX_ALTERNATION}}` | step 4 prefixes joined with `\|`, e.g. `FR\|CL\|SV\|NF` — used in the lefthook reminder regex |
| `{{PROJECT_UPPER}}` | project name upper-cased, `-` → `_` (CMake cache variables) |

Coverage-dependent slots, all filled from the floor chosen in step 4 — and all removed entirely, leaving no dangling clause, when the user chose "none":

| Placeholder | With a floor of N | With no floor |
|---|---|---|
| `{{COVERAGE_FLOOR}}` | `N` | — (coverage command dropped) |
| `{{COVERAGE_LINE}}` | `- Coverage floor N% of lines, CI-gated on every push and PR. A floor, not a target — never inflate it with tests that execute code without asserting.` | empty |
| `{{COVERAGE_GAUNTLET_WORD}}` | `/coverage` | empty |
| `{{COVERAGE_PLAN_CLAUSE}}` | `; expected coverage impact` | empty |
| `{{COVERAGE_STAGE_CLAUSE}}` | `, coverage ≥ N%` | empty |
| `{{COVERAGE_PR_CLAUSE}}` | `, the coverage number` | empty |
| `{{COVERAGE_CONTRIB_LINE}}` | `Line coverage must stay at or above **N%**, enforced in CI. Coverage is a floor, not a goal — never pad it with tests that execute code without asserting on it.` | empty |

Tracker-dependent slots (choice + branch from step 4b):

| Placeholder | GitHub | Jira (MCP) | Jira (credentials) | Filesystem | None |
|---|---|---|---|---|---|
| `{{ISSUE_WORKFLOW}}` | `issue-github.md` | `issue-jira-mcp.md` | `issue-jira-credentials.md` | `issue-filesystem.md` | `issue-none.md` |
| `{{CLOSES_CLAUSE}}` | `, ` + `` `Closes #<issue>` `` | `, references <ISSUE-KEY>` | `, references <ISSUE-KEY>` | empty | empty |

All five fragment files live in `templates/fragments/`.

**Substitute only the placeholders named above.** A GitHub Actions expression like `${{ matrix.python-version }}` inside a CI block is not a placeholder — copy it through verbatim.

Do **not** create the language manifest (`Cargo.toml`, `package.json`, …) or any source file. Scaffolding a project skeleton is the first task *through* the workflow, not part of setting it up — gate 1 owns the first behavior.

## 8. Remove the template scaffolding

Delete `templates/`, `.claude/skills/project-init/` and `.claude/skills/init-workspace/`. Keep `.claude/agents/`, remaining `.claude/skills/`, `.claude/tasks/` and `.claude/settings.json` — the workflow uses them. Do not delete `.git`, do not commit — leave the working tree dirty so the user reviews the diff.

## 9. Report and hand over

State what was created, overwritten and deleted, then the three things the user does next:

1. Review the diff and commit the scaffolding on `main` (this is the one commit that legitimately lands on `main` without going through the gates).
2. Install the hook runner if lefthook was generated (`lefthook install`).
3. Start the first feature: describe it, and the agent opens **gate 1** — the spec diff — before any code.

Then stop. Do not roll into the first feature.
