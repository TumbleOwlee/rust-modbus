---
name: spec-feature
description: Drive one behavior change through the repo's gated spec-driven TDD workflow — spec diff, tracking issue, implementation plan, worktree implementation, independent review, PR. Use when starting a feature, fix, or any change to observable behavior in a repo whose AGENTS.md defines these gates.
---

# Spec-driven feature run

Orchestrates one behavior change end to end. `AGENTS.md` is the authority — this
skill is the procedure for running it with worktrees and subagents. Where the two
disagree, `AGENTS.md` wins.

Read `AGENTS.md` before anything else.

## Does this even trigger?

Behavior change of any size → all gates. Refactor, rename, perf work with
identical semantics, tests, docs → no gates, just do the work. Size sets the
number of stages, never whether the gates exist. If unsure, ask: *does this
change what the software is required to do?*

Check `.claude/tasks/` first. Cards outside `open/` and `done/` mean an earlier
run was interrupted — go to *Resume an interrupted run* at the bottom instead of
starting something new.

## 0. Branch, worktree, parent card

```sh
git worktree add .claude/worktrees/<slug> -b <type>/<slug> main
```

Worktrees live under `.claude/worktrees/`, inside the project directory — an agent
confined to the project root can still reach them. That path is gitignored, so the
worktree never shows up as untracked content in the main checkout.

One worktree per issue, per agent. Two agents in one checkout interleave commits.
Everything below happens in that worktree.

Then create the parent card `.claude/tasks/open/<slug>.md` and the artifact
directory `.claude/tasks/artifacts/<slug>/`. The board lives in the **main
checkout**, never in a worktree; every path you hand an agent is absolute.

## 1. Gate 1 — spec diff

Spawn `spec-planner` with: the ask, the affected area(s), and the worktree path.

It returns the normative text. **You verify before showing it to the user:**

- Quoted requirement text matches the file on disk.
- Appended IDs are genuinely unused — grep all of `docs/specs/`.
- Nothing contradicts an existing requirement or `edge-cases.md` entry.
- It proposes what was actually asked, no more.

Then present it and **stop for approval**.

On approval: write the approved text to `artifacts/<slug>/spec-diff.md`, record
`gate1: approved <date>` on the parent card, append the approval to its log.

## 2. Gate 1b — tracking issue

Search existing issues (open and closed) for the same goal. Reuse what exists;
never open a second. Otherwise draft the issue per `AGENTS.md` — self-contained,
full normative text beside every ID, `##` sections, goal and normative changes
only, no implementation detail — and **stop for approval** before creating it.

Skip entirely if the repo has no tracker; the goal then lives in the PR body.
Record `issue: <n>` on the parent card when there is one. The card never copies
the goal text — the issue owns that.

## 3. Write the spec

Land the approved text in the working tree. Normative text only — nothing marked
unfinished. This is the first stage and the first commit.

## 4. Gate 2 — implementation plan

Spawn `spec-planner` again with the approved spec. It returns stages, numbered
file-level steps per stage, the dependency tree (per stage: what it depends on,
what files it touches), the ID → test table, the Verification method, expected
commits.

Verify against the approved spec, and verify the tree itself: two stages the plan
calls independent must not touch the same file, and every stage's dependencies
must actually produce what it consumes. A wrong tree surfaces as a merge conflict
three agents later.

Present the plan with the waves it implies — "stages 2, 3, 4 can run at once" —
then **ask, in the same approval, how to implement it**:

- **Sequential** (default) — one agent, stages in order.
- **Parallel** — the user names the maximum number of agents running at once.

Take the number from the user; never derive it from the tree. No answer means
sequential.

On approval, lay out the board:

- write the plan to `artifacts/<slug>/plan.md`,
- record `gate2` and `mode: sequential | parallel(N)` on the parent card and move
  it to `inprogress/`,
- create `open/<slug>.s<n>.md` per stage, copying `files` and `blocked-by`
  straight from the dependency tree,
- parallel: create `open/<slug>.w<n>.md` per wave. Sequential: one wave-gate card
  for the whole run.

## 5. Implement

**Sequential.** Spawn one `spec-implementer` with the approved spec, the approved
plan, the worktree path, and the absolute paths of its stage cards. It works stage
by stage under TDD on the feature branch, commits every green stage, and moves
each card `open` → `inprogress` → `inreview`. You verify and move it to `done` —
nothing to merge, so `done` means committed and verified.

**Parallel.** Work in waves, each wave at most the approved agent count:

1. Take the runnable stages — every `blocked-by` id in `done/` — up to the cap.
   Move the wave-gate card to `inprogress/`.
2. Per stage, one worktree and one branch off the feature branch as it stands
   now:

   ```sh
   git worktree add .claude/worktrees/<slug>-<n> -b <type>/<slug>-<n> <type>/<slug>
   ```

3. Spawn one `spec-implementer` per worktree, each given **only its own stages**,
   the full approved spec, the plan, and the absolute path of its own card.
4. Wait for the whole wave; each agent leaves its card in `inreview/`. Per card:
   merge the branch into the feature branch, re-run the gauntlet on the merged
   result, move the card to `done/`, remove the worktree.
5. All stages done → wave gate to `inreview/`: spawn `spec-reviewer` over the
   wave's accumulated diff on the feature branch. Clean → wave gate to `done/`,
   next wave starts with no approval prompt. Any finding → stop, report, move the
   implicated stage cards back to `inprogress/` with the finding appended.

A merge conflict inside a wave means the dependency tree was wrong — stop, report
it, re-plan. Do not hand-resolve and carry on; the tree is now lying about
everything downstream too.

When an implementer reports back: **its report is not verification.** Re-run the
full gauntlet from `AGENTS.md` yourself — in its worktree when sequential, on the
merged feature branch after each wave when parallel — read the code it describes,
check that ID citations sit beside their tests, and mutation-check any test that
looks like it was written after its implementation: break the implementation,
confirm the test fails.

If one stopped mid-plan, that is the plan being wrong or the spec being
ambiguous. Resolve it with the user; do not tell the agent to "just continue". In
a wave, the other agents' finished branches still merge — only the unfinished
stages get re-planned.

## 6. Reconcile the spec

Behavior that ended up differing from what gate 1 approved is normative and
re-opens gate 1: show the diff, state what forced it, get approval. Editorial
fixes (a wrong cross-reference, clumsy wording) need no approval. Report the
final spec diff either way.

## 7. Gate 3 — independent review

Move the parent card to `inreview/`. Spawn `spec-reviewer` — a different agent
than the implementer, always — with the diff base, the artifact directory, and the
worktree path. It appends findings to `artifacts/<slug>/review.md`, each keyed to a
stage id. Report its findings, apply the fixes that are clearly fixes, and raise
the ones that are decisions; a finding that reopens a stage moves that stage card
back to `inprogress/`. Re-run the gauntlet after any fix. **Stop for approval.**

## 8. Gate 4 — pull request

Run the plan's Verification method, report the outcome, then **ask whether to open
a PR** — the user may want their own manual run first. Once confirmed, draft title
and body, **stop for approval of that text**, then push and open it.

## 9. Merge and clean up

Squash merge to `main`, so the stage commits — including the spec commit that ran
ahead of its code — never reach `main`. Then:

```sh
git worktree remove .claude/worktrees/<slug>
git worktree list   # nothing under .claude/worktrees/ should survive
```

Per-wave worktrees are removed at the end of their wave; this is the sweep that
catches the ones a stopped agent left behind. Move the parent card to `done/`; no
card for this run should be left outside `done/`.

## Resume an interrupted run

Cards outside `open/` and `done/` with no agent running mean a session died
mid-run. Resume only when the user asks.

**Reconcile every card against git before acting on it.** The card is what an
agent intended; git is what happened, and the agent may have died in between:

| The card claims | Check | A disagreement means |
|---|---|---|
| a worktree | `git worktree list` | the card is stale |
| a branch | `git rev-parse` | the stage never started |
| `commit=<sha>` | the sha exists, on that branch | the commit never landed |
| `gauntlet=pass` | re-run it at that sha | the card overstated its state |
| stage `done` | `git branch --contains` vs the feature branch | it was never merged, and everything planned on top of it is planned on a lie |

Report the differences first. Where card and git agree, resume. Where they do not,
stop: a card lagging behind git is a forgotten move you may correct, but a card
claiming work git cannot show is never talked into being true.

After a clean reconcile, resume only work that needs no approval — respawn
implementers for approved stages, merge finished branches, run wave gates — and
halt at the first gate that needs the user. `gate1` and `gate2` approvals recorded
on the parent card stay valid; do not re-ask them.

## Standing rules

- Never commit to `main`.
- Never put a tool attribution trailer — `Co-Authored-By`, "Generated with" — in a
  commit message, PR body, issue or comment.
- Never skip a gate because the change is small.
- Never let an agent's self-report stand as verification.
- Never spawn more agents at once than gate 2 approved, and never run agents in
  parallel at all when gate 2 said sequential.
- Never let two concurrent agents share a worktree or a file.
- Never move a card to `done/` for work you performed yourself — `done` is the
  orchestrator's word, after merging and re-running the gauntlet.
- Never act on a card that git contradicts.
- Never fold an unrelated pre-existing spec/code disagreement into this work.
  Raise it as its own task.
