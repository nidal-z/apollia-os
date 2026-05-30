# sanitize-codebase changelog

All notable changes to this package are documented here. Format follows
Keep a Changelog 1.1.0, semver applies to the package version declared
in `manifest.toml`.

## [0.1.0] - 2026-05-30

### Added

- Director `sanitize-codebase-director` (L2, interval triggered, state
  machine 8 steps: INIT / LOAD_DATASOURCES / LOAD_MANIFEST / PICK_BATCH
  / DISPATCH_SONAR / DISPATCH_COMMENTS / VERIFY_HARNESS /
  PERSIST_PROGRESS / NOTIFY / DONE).
- Worker `sonar-cleanup-worker` exposing skill `sanitize.clean_file`,
  driven by MCP `sonarqube/search_issues`, behavior preserving edits
  gated via cargo / ruff / pnpm.
- Worker `comment-sanitize-worker` exposing skill
  `sanitize.translate_comments`, line-based parser per language,
  diff-only-touches-comments invariant enforced before writing.
- Datasources: `rules.yaml` (priority table + harness per language),
  `pool.yaml` (file globs + batch size), `exclusions.yaml` (do-not-touch
  list, internal ref map, forbidden chars).
- Jinja2 templates: `sonar-fix-prompt.md.j2`,
  `comment-translate-prompt.md.j2`, `batch-summary.md.j2`.
- TypedDict schemas for the A2A payloads (no `from __future__ import
  annotations` to preserve `__required_keys__`).
- Persona rules in `memory/APOLLIA.md` (non-negotiable harness gate,
  code/comment pass separation, no self-sanitize).
- Eval suite: `eval/cases.jsonl` (5 cases) + `eval/run-eval.py` harness.
- Replay-and-compare procedure in `eval/replay.md`.
- Interval trigger declared disabled by default in `manifest.toml`
  (operator must explicitly enable after wiring the local LLM and the
  SonarQube MCP).
