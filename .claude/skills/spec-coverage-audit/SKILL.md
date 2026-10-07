---
name: spec-coverage-audit
description: Find code with no requirement covering it, and get an approved spec diff (new requirements, and new capability areas if needed) that catches docs/specs up to what the code already does. Requires a spec directory + routing table to already exist — refuses otherwise. Does not check for stale/untested requirements (spec with no citing test) — only reports that gap and asks whether to handle it. Use when the user asks to "find missing specs", "audit spec coverage", "what's undocumented", or "backfill specs for this code".
---

# Spec coverage audit

**Concise, compact, facts only.**

Direction covered: **code → no spec**. The reverse (**spec → no test**,
stale/orphaned requirement) is out of scope for this run — step 7 asks the
user about it, doesn't act on it.

Guard rails: never write a requirement without explicit approval first;
never fabricate — every proposed entry must trace to code you actually
read, not a guess.

## 0. Check requirements

An `AGENTS.md` (or `CLAUDE.md`) routing table pointing at a spec directory
must already exist. Missing → stop, tell the user a governed spec directory
needs to be set up first. Do not create the spec directory yourself — this
skill only ever adds requirements *within* an already-governed structure.

A second, conditional requirement — a sibling `spec-scaffold-init/`
skill directory with its bundled area templates — is only needed if this
run ends up proposing a brand-new area (step 4). Checked at the point it's
actually used, step 6, not here; its absence doesn't block gap-detection
within areas that already exist.

## 1. Scope

Accept an optional area or path argument to narrow the run. No argument →
every area in the routing table.

## 2. Map each area to source

Per area in scope: guess its source directory (area name vs `src/<area>`-
style match, or the routing table's own wording for what that area
"covers"), then confirm or correct the guess via one `AskUserQuestion`
before reading anything — auditing the wrong directory silently is worse
than one extra question.

## 3. Read and compare — per area

This is a semantic read, not a grep for citation markers: a citation-grep
proxy only catches tests missing an ID, and misses code with *no test at
all*, which is exactly the gap that matters most here.

For the confirmed source directory:

1. Read the area's `requirements.md` (and `edge-cases.md`, so a documented
   intentional gap isn't re-flagged as missing).
2. Read the source code in that area.
3. Reason about what observable behavior exists vs. what's stated as a
   requirement statement. Flag behavior with nothing covering it.

For anything that doesn't fit any area in scope (or any area at all): note
it separately — handled below in this same step, not folded silently into
the nearest area.

## 4. Draft the diff

- **Gaps within an existing area**: new `<PREFIX>-R-nnn` entries (indicative statements, README rule 4),
  one rule each (`docs/specs/README.md` rule 9 — two independent behaviors
  are two IDs, never one longer line), next free number for that area's prefix (append-only — check the highest
  existing number in that `requirements.md`, never reuse or renumber).
  Testable, observable-outcome wording, e.g.
  `**<PREFIX>-R-001** — The <subject> <observable outcome, indicative> when <condition>.`
- **Code fitting no existing area**: propose a new area inline —
  directory name (lowercase, short), one-line "covers" description, and a
  unique requirement ID prefix (two letters + `-R-`, not colliding with any
  existing prefix or `NF-R-*`) — then draft its first requirements the same
  way.

## 5. Get approval

Present the full draft — grouped by area, new areas called out separately —
for one explicit approval: derive the spec text, stop before writing.
No tracking issue, no commit — code already exists, so there's no
implementation step to track.

Rejected/edited items: adjust and re-confirm before writing.

## 6. Write

Any new area proposed in step 4: before writing it, check that a sibling
`spec-scaffold-init/templates/area/` directory exists (relative to this
skill's own directory). Missing → stop just the new-area part, tell the
user new-area scaffolding needs `spec-scaffold-init` present alongside this
skill; still write the approved requirements for existing areas below.
Present → read its `requirements.md.tmpl`/`edge-cases.md.tmpl` (and
`api-contract.md.tmpl`/`data-contract.md.tmpl` if relevant), substitute
placeholders, write the new area's files, and append a routing-table row
for it:

```
| <area covers> | [`<area>`](<spec-dir>/<area>/) | `<PREFIX>-R-*` |
```

Then append the approved requirement entries to each `requirements.md`.

Report what was written (files touched, requirement IDs added, any new
area created). Then stop — no commit, no issue, no PR. The user reviews and
commits manually.

## 7. Ask about the other direction

Last thing before ending the run — ask the user: should a follow-up also
report requirements with no citing test (spec → no test, stale/unverified),
and if so, should that be added to this skill or built as a separate one?
Record the answer if they give a clear direction; don't act on it in this
run either way.
