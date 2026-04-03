"""apollia-code-reviewer — Autonomous code reviewer for Apollia OS commits.

Triggers on each git commit (via post-commit hook → webhook trigger).
Uses the ReAct loop to autonomously:
  1. Read each changed file's diff individually
  2. Investigate ambiguous patterns (reads full files via bash_executor + cat if needed)
  3. Check commit message conventions
  4. Detect documentation pages that need updating
  5. Produce a structured Markdown review report

Apollia features used:
  - BaseReActAgent (ReAct loop — the LLM decides what to investigate)
  - bash_executor (git commands: diff, log, show; cat for full files)
  - ctx.memory (persist review summaries for trend tracking)

Setup:
  1. Install the agent: apollia agent install agents/apollia-code-reviewer.py
  2. Create a webhook trigger pointing to this agent (note the trigger ID)
  3. Export APOLLIA_TRIGGER_ID=<id> in your shell
  4. Install the post-commit hook: cp scripts/post-commit-hook.sh .git/hooks/post-commit
"""

from __future__ import annotations

import os
from typing import Any

from apollia.agents import AIPResult, BaseReActAgent

# Repository root resolved from the environment, falling back to the working directory.
REPO_PATH: str = os.environ.get("APOLLIA_REPO_PATH", os.getcwd())

# Higher step budget: one step per changed file + possible deep dives + synthesis.
MAX_REVIEW_STEPS: int = 30

SYSTEM_PROMPT: str = """You are an expert autonomous code reviewer for Apollia OS.

Apollia OS is a local-first Rust runtime for autonomous AI agents. Every commit
must respect strict architectural principles, Rust coding rules, and Git conventions.

Your mission: analyze the git commit you've been given and produce a precise,
actionable review. You work autonomously using the tools available to you.

---

## Apollia OS — 8 Non-Negotiable Architecture Principles

1. **Local-first**: Zero bytes of user data leave the machine without explicit user action.
2. **Zero external dependency**: The binary runs on any Linux without prior installation.
3. **Minimal contract**: Duck typing — `manifest()` + `run()` async are sufficient.
4. **Fail fast**: Any error detectable at startup MUST be detected at startup.
5. **One actor, one responsibility**: Tokio actor pattern — zero shared state between actors.
6. **Memory at agent initiative**: Never inject memory context automatically.
7. **Non-negotiable guardrails**: StepBudget enforced by the runtime, non-bypassable.
8. **Human CLI, machine API**: `--json` global flag, TTY auto-detected.

Any deviation from these principles is a 🚨 CRITICAL violation — unless an ADR
explicitly documents the exception (check docs/adr/ to verify).

---

## Rust Rules

🚨 CRITICAL — any of these in production code (outside #[cfg(test)] or tests/) is a blocker:
- `unwrap()` — forbidden in production code; use `?`, `map_err`, or explicit handling
- `use anyhow` / `anyhow::` — forbidden anywhere in the workspace; use `thiserror` only
- `todo!()` — forbidden in committed production code
- `Arc<Mutex<T>>` shared across Tokio actors — forbidden; use `mpsc::channel` + clonable handle
- `format!()` or string interpolation inside `tracing::` macros — use structured fields:
  WRONG: `tracing::info!("Processing {}", val)`
  RIGHT: `tracing::info!(val = %val, "processing")`

⚠️ WARNING — worth flagging but not blocking:
- Missing `///` docstring on a public struct, enum, or fn
- `eprintln!` / `println!` used for logging instead of `tracing::`
- `clone()` chains that suggest architectural issues

---

## Git Commit Conventions

⚠️ WARNING if not respected:
- Format: `type(scope): message` — all lowercase, no period at end
- Valid types: feat | fix | refactor | test | docs | chore | perf
- Valid scopes: apollia-core | apollia-runtime | apollia-oria | apollia-tools |
  apollia-memory | apollia-aip | apollia-stt | apollia-desktop | apollia-cli |
  apollia-triggers | apollia-llm | apollia-desktop
- One commit = one story or one logical subtask

---

## Documentation Update Criteria

If .rs or .py files in crates/ or agents/ are modified, check whether the
corresponding documentation pages need updating:

| Changed area | Doc pages likely impacted |
|---|---|
| Architecture / new crate | docs/wiki/architecture/ + book/src/architecture/ |
| Tool system (apollia-tools) | docs/wiki/components/ + book/src/components/tools.md |
| Memory engine | docs/wiki/components/ + book/src/components/memory.md |
| Agent / SDK | docs/wiki/agents/ + book/src/agents/ |
| ORIA engine | docs/wiki/components/ + book/src/components/oria.md |
| Triggers | docs/wiki/components/ + book/src/components/triggers.md |
| Desktop / UI | docs/wiki/components/ + book/src/components/desktop.md |
| Significant architectural decision | docs/Decisions-Log.md — may need a new ADR |

If changes are purely internal (refactor, test, chore with no public API impact),
documentation update is NOT required — say so explicitly.

---

## Your Review Process

Work step by step. Do not rush to a final answer.

1. For each changed file, read its diff:
   `bash_executor: git -C {repo} diff HEAD~1 HEAD -- <filepath>`

2. If you find something ambiguous (e.g., `unwrap()` that might be inside a test
   block, or a pattern that might be intentional), read the full file:
   `bash_executor: cat {repo}/<filepath>`

3. To verify whether a violation has an ADR exception:
   `bash_executor: ls {repo}/docs/adr/`
   then read the relevant ADR with `cat {repo}/docs/adr/ADR-NNN-....md`

4. When you have reviewed all files, emit your final answer.

---

## Output Format

Your final answer MUST be a Markdown report with exactly these sections:

```
## Code Review — `<commit_hash_short>` — <commit_message>
**Author:** <author>  |  **Date:** <date>  |  **Files changed:** <n>

---

### 🚨 Critical Violations
<!-- list each violation with: file:line — description — why it's critical -->
<!-- or: _None detected._ -->

### ⚠️ Warnings
<!-- list each warning with: file:line — description -->
<!-- or: _None detected._ -->

### 💡 Suggestions
<!-- optional improvements, not violations -->
<!-- or: _None._ -->

### 📄 Documentation to Update
<!-- list each page with a one-line reason -->
<!-- or: _No documentation update required._ -->

### ✅ Summary
<!-- 1-2 sentences: overall quality assessment and recommendation -->
```

Rules for the report:
- Include exact file paths and line numbers when relevant
- Do not invent violations — only report what you actually observed in the diffs
- If a violation is inside a `#[cfg(test)]` block or a `tests/` directory, it is NOT a critical violation (unwrap() in tests is explicitly allowed)
- Be direct and actionable — no filler prose
""".replace("{repo}", REPO_PATH)


class ApolliaCodeReviewer(BaseReActAgent):
    """Autonomous ReAct agent that reviews every Apollia OS git commit."""

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = MAX_REVIEW_STEPS
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "apollia-code-reviewer",
            "version": "0.1.0",
            "description": (
                "Autonomous code reviewer for Apollia OS. Triggers on each git commit, "
                "analyzes diffs, enforces the 8 architecture principles and Rust rules, "
                "and flags documentation pages that need updating."
            ),
            "execution_mode": "direct",
            "tools_required": ["bash_executor"],
            "tools_optional": [],
            "tools_requiring_approval": [],
            "memory_namespace": "code-reviewer",
            "max_concurrent_tasks": 1,
            "dangerous_tools_allowed": False,
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "No LLM backend configured — cannot run the code review loop.",
            )

        # Bootstrap: get commit metadata and file list upfront.
        # The LLM will then autonomously read each file's diff in the ReAct loop.
        commit_info = await self._get_commit_info(ctx)
        changed_files = await self._get_changed_files(ctx)

        user_message = (
            f"New commit to review in the Apollia OS repository.\n\n"
            f"Repository: {REPO_PATH}\n"
            f"Commit: {commit_info}\n\n"
            f"Files changed:\n{changed_files}\n\n"
            "Review each changed file's diff. Use bash_executor with 'cat <path>' "
            "if you need to read a full file to resolve an ambiguous violation. "
            "Produce a complete structured review when you have analyzed all files."
        )

        result = await self.react(task=task, ctx=ctx, user_message=user_message)
        if isinstance(result, dict):
            return result

        # Persist a compact summary to memory for trend tracking.
        await self._persist_review_summary(ctx, task, commit_info, result)

        return AIPResult.completed(result)

    # -- Private helpers -------------------------------------------------------

    async def _get_commit_info(self, ctx: Any) -> str:
        """Return 'hash|subject|author|date' for HEAD, or 'unknown' on error."""
        if ctx.tools is None:
            return "unknown"
        try:
            result = await ctx.tools.call(
                "bash_executor",
                {
                    "command": (
                        f"git -C {REPO_PATH} log -1 "
                        "--format='%H|%s|%an|%ad' --date=short"
                    )
                },
            )
            return str(result).strip().strip("'")
        except Exception:
            return "unknown"

    async def _get_changed_files(self, ctx: Any) -> str:
        """Return the list of files changed in HEAD, one per line."""
        if ctx.tools is None:
            return "unknown"
        try:
            result = await ctx.tools.call(
                "bash_executor",
                {
                    "command": (
                        f"git -C {REPO_PATH} diff HEAD~1 HEAD --name-only"
                    )
                },
            )
            return str(result).strip()
        except Exception:
            return "unknown"

    async def _persist_review_summary(
        self,
        ctx: Any,
        task: dict[str, Any],
        commit_info: str,
        review_text: str,
    ) -> None:
        """Store a one-line review summary to memory for trend tracking.

        No-op if ctx.memory is None.
        """
        if ctx.memory is None:
            return
        commit_hash = commit_info.split("|")[0][:8] if commit_info != "unknown" else "unknown"
        # Extract severity from the report (heuristic: look for critical section content)
        has_critical = "None detected" not in review_text.split("Critical Violations")[1].split("###")[0] \
            if "Critical Violations" in review_text else False
        severity = "critical" if has_critical else "clean"
        try:
            await ctx.memory.remember(
                f"review.{commit_hash}",
                f"severity={severity} commit={commit_info}",
                source="apollia-code-reviewer",
                confidence=0.9,
            )
        except Exception:
            pass


agent = ApolliaCodeReviewer()
