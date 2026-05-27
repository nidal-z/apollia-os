# CLAUDE.md, Claude Code overlay

> This file is the Claude-Code-specific overlay for the Apollia OS repo.
> It is short by design.
>
> **The authoritative rulebook is `AGENTS.md` at the repo root and
> `docs/agents/*.md`.** Read those first. This file adds only the
> Claude-Code-specific session context.

---

## Where to read first

1. `AGENTS.md` (root), the standard entry point for any LLM coding
   assistant.
2. `docs/agents/INDEX.md`, the navigation map for the long-form rulebook.
3. The nearest sub-`AGENTS.md` for the area you are about to touch
   (`crates/<crate>/AGENTS.md` or `sdk/AGENTS.md` or
   `crates/apollia-desktop/ui/AGENTS.md`).
4. `docs/agents/FORBIDDEN.md` before every commit.

Do not duplicate content here. Anything generic about the project lives
in `AGENTS.md` and `docs/agents/`.

---

## Claude Code skills available in this repo

Project-specific skills configured under `.claude/skills/` :

- **apollia-story** : create or refine a User Story in
  `docs/internal/STORIES/sprint-N/story-NNN.md`.
- **apollia-sprint** : plan or close a sprint in
  `docs/internal/STORIES/sprint-N/plan.md`.
- **apollia-adr** : generate a new ADR in `docs/adr/ADR-NNN.md`.
- **apollia-doc-setup** : initialize `docs/` and `book/` mdBook scaffold
  (one-shot, first time only).
- **apollia-doc-sync** : update docs after a sprint, story, architectural
  change, or diagram update.
- **apollia-doc-sync-diff** : sync book / wiki / help from a git commit
  range via the routing table.
- **apollia-doc-research** : internal technology watch (MCP / A2A,
  competitors, pivot signals).

Use them when their description matches the task. Never invent a skill
name.

---

## Auto-memory

The auto-memory system in `~/.claude/projects/-Users-nidalzoumita-dev-apollia-v2/memory/`
is the persistent user-level memory. Entries there override anything
this file says when the topic matches.

Examples of memory entries that govern this project (non-exhaustive) :
no em-dash in prose, no `Co-Authored-By: Claude` trailer, exhaustive
audit preference, frontend legacy/canon doctrine, designer briefs
content-only, etc.

---

## Current release context

Source of truth : `docs/internal/release/plan-release.md` (J-by-J plan).

| Item | Value |
|---|---|
| Target release | `v0.1.0-preview`, public repo |
| D-Day | mar 3 juin 2026 |
| Public repo passage | lun 2 juin 2026 |
| Doc / spec sprint (this corpus) | livré dans la branche courante |

Status files in `docs/internal/release/` :

- `plan-release.md`, daily plan, source of truth.
- `REPO-STATE.md`, public repo passage state.
- `CLI-STATE.md`, CLI completion state (closed).
- `HELP-STATE.md`, help corpus state.
- `DOCS-STATE.md`, this doc + master spec LLM sprint state.

These files are gitignored. They are the operational source of truth
for the current phase, not the public-facing roadmap.

---

## Internal references vs public docs

| Path | Status |
|---|---|
| `docs/internal/` | gitignored, internal planning |
| `docs/adr/` | committed, English, append-only |
| `docs/book/` | committed (mdBook), French, pedagogical |
| `docs/wiki/` | committed, English (post-L2b), reference |
| `docs/help/` | committed, French, operator |
| `docs/agents/` | committed, English, LLM rulebook |
| `sdk/apollia/stubs/` | committed, type contract for `Ctx` |

Never reference `docs/internal/*` from public files. Never bake daily
state into committed code or docs. Use `docs/internal/release/*` for
that.

---

## When this file goes stale

Update the release context block above on every release boundary
(release day, after a major milestone). Do not let it drift more than a
sprint behind the actual state.

If a rule below conflicts with `AGENTS.md` or `docs/agents/*.md`, the
agents corpus wins. Surface the conflict and update this file.
