---
name: spec-scaffold-init
description: Add a governed spec directory (capability areas, requirement-ID convention, routing pointer) to a project — appends to an existing AGENTS.md/CLAUDE.md, or creates a minimal one if neither exists. Does not touch build/test/lint, CI, hooks, or issue-tracker setup. Use when the user wants "just the spec directory", "spec structure only", or "add docs/specs without the full workflow".
---

# Spec scaffold init

**Concise, compact, facts only.**

Gates 2-4 (implementation plan, worktree, review) stay out; step 6's note covers only how a spec change itself gets proposed and approved.

Self-contained: every source file this skill writes lives under this
skill's own directory (`templates/`, `scripts/`). Copy this skill's folder
alone into any project and it still works.

Guard rails: ask, never guess (`AskUserQuestion` for every unknown below) —
never fabricate requirement text (every generated file ships empty/stub,
real requirement statements land later, through the approval step in step 4) —
never overwrite a file without asking keep/overwrite/merge first.

## 0. Check requirements

Verify this skill's own `templates/` and `scripts/` directories exist next
to this `SKILL.md` (same directory). Missing either → this skill's folder
was copied incompletely; stop and tell the user to copy the whole
`spec-scaffold-init/` directory, not just `SKILL.md`, then retry.

## 1. Detect existing agent-instructions file

Check for `AGENTS.md`, else `CLAUDE.md`. Found → that file is the append
target for step 5. Neither exists → step 5 creates a new, minimal
`AGENTS.md` instead of appending.

## 2. Ask the spec directory location

`AskUserQuestion`, default `docs/specs` — some projects already use `docs/`
for something else, or want specs kept elsewhere. Every path in the steps
below is `<spec-dir>`, substituted with the confirmed answer, never a
hardcoded `docs/specs`.

## 3. Ask the capability areas

One `AskUserQuestion` round, 2-6 areas, each with —

- a directory name (lowercase, short: `frame`, `client`, `transport`),
- a one-line "covers" description for the routing table,
- a **requirement ID prefix**: two letters + `-R-` (`FR-R-nnn`). Must be
  unique, must not collide with `NF-R-*` (reserved for non-functional
  requirements).

If the user doesn't know the areas yet: don't invent them — use the
"populate as you go" fallback in step 4 below (only
`non-functional-requirements.md` created, TBD row in the routing table).
Workflow still functions; areas get added later the same way.

Also ask, same round: **per-area starting files** — default
`requirements.md` + `edge-cases.md`; add `api-contract.md` for anything with
a public surface, `data-contract.md` for anything with a wire or file
format.

## 4. Confirm before writing

Show a compact summary — spec directory location, areas with prefixes and
starting files, files to create, file to append to (or "new minimal
AGENTS.md") — and get a yes. This is the only approval gate in this skill;
after it, write everything without further prompting.

## 5. Write the files

Bundled templates live under this skill's own `templates/` directory (paths
below are relative to this `SKILL.md`) — read each, substitute
placeholders, write to `<spec-dir>/...`:

| Bundled template | Output |
|---|---|
| `templates/README.md.tmpl` | `<spec-dir>/README.md` |
| `templates/non-functional-requirements.md` | `<spec-dir>/non-functional-requirements.md` (static, copy as-is) |
| `templates/area/requirements.md.tmpl` | `<spec-dir>/<area>/requirements.md` — once per area |
| `templates/area/edge-cases.md.tmpl` | `<spec-dir>/<area>/edge-cases.md` — once per area |
| `templates/area/api-contract.md.tmpl` | `<spec-dir>/<area>/api-contract.md` — only areas that asked for it |
| `templates/area/data-contract.md.tmpl` | `<spec-dir>/<area>/data-contract.md` — only areas that asked for it |

`{{PROJECT_NAME}}` in `README.md.tmpl`: infer silently from the repo
directory name — cosmetic title text, not worth an `AskUserQuestion`.
`README.md.tmpl` carries no area table — it points at the routing table
step 6 appends, the single copy of the area list. `{{AREA_TITLE}}`, `{{AREA_COVERS}}`,
`{{AREA_PREFIX}}` in the per-area templates: from step 3's answers for that
area.

No areas known yet (fallback from step 3): write only
`<spec-dir>/non-functional-requirements.md` and `<spec-dir>/README.md`,
with a "populate as you go" note and a TBD row in the routing table
appended in step 7.

Copy this skill's bundled `scripts/extract-id.sh`,
`scripts/extract-section.sh`, `scripts/list-sections.sh`,
`scripts/token-rank.sh` into the target project's `.claude/scripts/`,
`chmod +x`. Skip any that already exist at the destination.

Grep every written file for `{{` before continuing — a leak means a missed
placeholder.

## 6. Append the routing section

Append to the target file from step 1 (or write the new minimal file if
neither existed) a section containing exactly three things, nothing more:

1. **Routing table** — one row per area, plus the fixed
   `non-functional-requirements.md` row:

   ```
   | Task touches | Read | ID prefix |
   |---|---|---|
   | <area covers> | [`<area>`](<spec-dir>/<area>/) | `<PREFIX>-R-*` |
   | Cross-cutting (platforms, performance, security, versioning, testing) | [`non-functional-requirements.md`](<spec-dir>/non-functional-requirements.md) | `NF-R-*` |
   ```

2. **ID citation convention** — requirement IDs are stable and append-only,
   never renumbered or reused once retired; cite them in commits/PRs/tests.
3. **Minimal spec-handling note**, self-contained: before adding or
   changing a requirement in `<spec-dir>/`, propose the diff and get
   explicit approval from whoever owns the project, then write it. Nothing
   about implementation plans, worktrees, or review.

If step 1 found nothing and this step is creating a new `AGENTS.md`: the
three items above are the entire file (plus a one-line title/repo
description at the top). Do not add any other section.

## 7. Report and hand over

State what was created and what was appended/created for the
agent-instructions file. Then stop. Do not create requirements, do not open an issue, do not commit.
