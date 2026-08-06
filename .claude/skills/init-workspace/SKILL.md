---
name: init-workspace
description: Entry point for initializing a fork of the spec-driven TDD template — the same bootstrap as /project-init, under a name that does not collide with the built-in /init. Use for "initialize the repo", "set up this workspace", or /init in a repo that still contains templates/.
---

Alias. Repo is a fork of a spec-driven TDD template — initializing it means bootstrapping the workflow, not summarising a codebase that doesn't exist yet.

Read [`../project-init/SKILL.md`](../project-init/SKILL.md) and follow it end to end.

If `templates/` no longer exists, the bootstrap already ran. Say so, fall back to ordinary behavior: read the code, update `CLAUDE.md`/`AGENTS.md` — without touching the workflow gate text.
