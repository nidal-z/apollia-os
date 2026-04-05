"""apollia-review — Structured code and plan review agent for Apollia OS.

Accepts a diff source (PR number, diff file, task ID, or falls back to
``git diff HEAD~1``) and produces a ``ReviewReport`` with a 0-100 score,
typed issues, and a textual summary.

Standards applied:
- Correctness : IEEE 830 (SRS) + unit-test criteria
- Security    : OWASP Top 10 (2021), CWE Top 25
- Quality     : ISO/IEC 25010:2023 (SQuaRE)
- Performance : algorithmic complexity, language-specific anti-patterns

Project-specific rules are injected dynamically from APOLLIA.md when present.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field, asdict
from typing import Any

# ─── Domain types ─────────────────────────────────────────────────────────────

@dataclass
class ReviewIssue:
    """A single issue surfaced during review."""

    severity: str   # "error" | "warning" | "info"
    category: str   # "correctness" | "convention" | "performance" | "security" | "coverage"
    message: str
    location: str | None = None


@dataclass
class ReviewReport:
    """Structured output of the review agent."""

    score: int      # 0 – 100
    issues: list[ReviewIssue] = field(default_factory=list)
    summary: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "score": self.score,
            "issues": [asdict(i) for i in self.issues],
            "summary": self.summary,
        }


# ─── Constants ────────────────────────────────────────────────────────────────

_AGENT_NAME = "apollia-review"
_AGENT_VERSION = "0.1.0"
_MAX_DIFF_CHARS = 30_000

_PUBLIC_STANDARDS = """\
Evaluate the code diff according to the following standards:

- **Correctness** : IEEE 830 requirements; unit-test completeness criteria.
- **Security**    : OWASP Top 10 (2021), CWE Top 25 Most Dangerous Weaknesses.
- **Quality**     : ISO/IEC 25010:2023 (SQuaRE) — maintainability, reliability.
- **Performance** : algorithmic complexity (Big-O), language-specific anti-patterns."""

_RESPONSE_FORMAT = """\
Respond with a single JSON object — no markdown fences, no prose outside the JSON.

Schema:
{
  "score": <integer 0-100>,
  "issues": [
    {
      "severity": "error" | "warning" | "info",
      "category": "correctness" | "convention" | "performance" | "security" | "coverage",
      "message": "<concise description>",
      "location": "<file:line>" | null
    }
  ],
  "summary": "<1-3 sentence summary>"
}

Rules:
- score 0   = critical failures blocking merge
- score 100 = clean, no issues
- Only report issues visible in the diff; do not invent violations."""


# ─── Manifest ─────────────────────────────────────────────────────────────────

def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest."""
    return {
        "name": _AGENT_NAME,
        "version": _AGENT_VERSION,
        "description": (
            "Structured code-review agent. Accepts a PR number, diff file, task ID, "
            "or falls back to git diff HEAD~1. Returns a JSON ReviewReport with score, "
            "issues (correctness / convention / performance / security / coverage), "
            "and a textual summary."
        ),
        "execution_mode": "direct",
        "inputs": {
            "task_id":     {"type": "string",  "required": False, "description": "Apollia task ID to review (plan review mode)."},
            "pr_number":   {"type": "integer", "required": False, "description": "GitHub PR number — uses `gh pr diff <n>`."},
            "diff_file":   {"type": "string",  "required": False, "description": "Path to a .diff / .patch file to analyse."},
            "review_type": {"type": "string",  "required": False, "description": "\"code\" (default) or \"plan\"."},
        },
        "tools_required": ["bash_executor"],
        "tools_optional": [],
        "tools_requiring_approval": [],
        "memory_namespace": "review",
        "max_concurrent_tasks": 4,
        "dangerous_tools_allowed": False,
    }


# ─── Entry point ──────────────────────────────────────────────────────────────

async def run(ctx: Any) -> dict[str, Any]:
    """Execute the code review and return a serialised ReviewReport."""
    task: dict[str, Any] = getattr(ctx, "task", {}) or {}
    inputs: dict[str, Any] = _extract_inputs(task)

    # Detect review type — "plan" when a task_id is given, "code" otherwise.
    review_type: str = inputs.get("review_type") or ("plan" if inputs.get("task_id") else "code")

    # Obtain the diff content.
    diff_content = await _obtain_diff(ctx, inputs)

    if not diff_content or not diff_content.strip():
        report = ReviewReport(score=100, summary="Aucun changement à analyser.")
        return report.to_dict()

    # Truncate to respect LLM context limits.
    diff_content = diff_content[:_MAX_DIFF_CHARS]

    # Build the system prompt (public standards + optional project rules).
    system_prompt = await _build_system_prompt(ctx, review_type)

    # Call the LLM.
    if ctx.llm is None:
        report = ReviewReport(
            score=0,
            summary="Erreur parsing: aucun backend LLM disponible.",
        )
        return report.to_dict()

    user_message = f"Diff to review:\n\n```diff\n{diff_content}\n```"

    try:
        response = await ctx.llm.complete(
            messages=[
                {"role": "system", "content": system_prompt},
                {"role": "user",   "content": user_message},
            ],
            temperature=0.1,
        )
        raw = response.content
    except Exception as exc:
        report = ReviewReport(score=0, summary=f"Erreur parsing: LLM call failed: {exc}")
        return report.to_dict()

    # Parse the LLM response as ReviewReport.
    return _parse_report(raw).to_dict()


# ─── Helpers ──────────────────────────────────────────────────────────────────

def _extract_inputs(task: dict[str, Any]) -> dict[str, Any]:
    """Pull review inputs from the AIP task payload."""
    # Support both flat dict and AIP parts-based payloads.
    raw_input = task.get("input", {})
    if isinstance(raw_input, dict):
        parts = raw_input.get("parts", [])
        if parts:
            for part in parts:
                if part.get("type") == "data":
                    data = part.get("data", {})
                    if isinstance(data, dict):
                        return data
        return raw_input
    return {}


async def _obtain_diff(ctx: Any, inputs: dict[str, Any]) -> str:
    """Return raw diff text from the appropriate source."""
    pr_number = inputs.get("pr_number")
    diff_file = inputs.get("diff_file")
    task_id = inputs.get("task_id")

    if pr_number is not None:
        return await _bash(ctx, f"gh pr diff {int(pr_number)}")

    if diff_file:
        return await _bash(ctx, f"cat {diff_file}")

    if task_id:
        # Retrieve the agent's execution plan from the runtime API as a diff proxy.
        return await _bash(
            ctx,
            f"curl -sf --unix-socket /tmp/apollia.sock "
            f"http://localhost/api/v1/tasks/{task_id} 2>/dev/null || echo ''",
        )

    # Fallback: diff against the previous commit.
    return await _bash(ctx, "git diff HEAD~1")


async def _bash(ctx: Any, command: str) -> str:
    """Run a shell command via the bash_executor tool and return stdout."""
    if ctx.tools is None:
        raise RuntimeError("bash_executor tool is unavailable: ctx.tools is None")
    result = await ctx.tools.call("bash_executor", {"command": command})
    if isinstance(result, dict):
        return str(result.get("output", result.get("stdout", "")))
    return str(result)


async def _build_system_prompt(ctx: Any, review_type: str) -> str:
    """Assemble the system prompt from public standards plus optional APOLLIA.md rules."""
    project_rules = ""
    try:
        apollia_md = await _bash(ctx, "cat APOLLIA.md")
        if apollia_md and apollia_md.strip():
            project_rules = (
                "\n\nProject-specific rules (from APOLLIA.md — these take precedence "
                "over all public standards above):\n" + apollia_md
            )
    except Exception:
        pass  # APOLLIA.md absent or unreadable — use public standards only.

    review_context = (
        "You are reviewing an Apollia OS execution plan (JSON). "
        "Evaluate correctness and coverage of the plan steps."
        if review_type == "plan"
        else "You are reviewing a code diff."
    )

    return "\n\n".join([
        review_context,
        _PUBLIC_STANDARDS + project_rules,
        _RESPONSE_FORMAT,
    ])


def _parse_report(raw: str) -> ReviewReport:
    """Parse the LLM JSON response into a ReviewReport.

    Returns a degraded report (score=0) on any parsing failure.
    """
    try:
        data = json.loads(raw.strip())
        score = int(data.get("score", 0))
        score = max(0, min(100, score))

        issues: list[ReviewIssue] = []
        for item in data.get("issues", []):
            if not isinstance(item, dict):
                continue
            issues.append(
                ReviewIssue(
                    severity=str(item.get("severity", "info")),
                    category=str(item.get("category", "correctness")),
                    message=str(item.get("message", "")),
                    location=item.get("location") or None,
                )
            )

        return ReviewReport(
            score=score,
            issues=issues,
            summary=str(data.get("summary", "")),
        )
    except Exception as exc:
        return ReviewReport(score=0, summary=f"Erreur parsing: {exc}")
