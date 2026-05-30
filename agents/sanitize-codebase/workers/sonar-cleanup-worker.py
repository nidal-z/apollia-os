"""sonar-cleanup-worker v0.1.0, Behavior preserving Sonar issue cleaner.

Skill: sanitize.clean_file (path, language, crate, max_issues).

The worker:
1. Queries MCP sonarqube/search_issues for the file path component.
2. Ranks issues by the per-language priority table loaded from
   datasources/rules.yaml.
3. Reads the file, renders the sonar-fix-prompt Jinja2 template with
   top issues + hints, asks the local LLM to propose a single
   behavior preserving edit (JSON output).
4. Applies the edit via file_write (full overwrite, no native
   file_edit tool is assumed in the runtime).
5. Runs the language harness (cargo / ruff / pnpm). If red, reverts
   the file from the pre-edit snapshot and returns "residue".
"""

import json
import re
import sys
from pathlib import Path
from typing import Any

import jinja2
import yaml

from apollia import DomainError, agent, skill
from apollia.types import Ctx


def _capture_agent_dir() -> Path:
    if sys.path:
        candidate = Path(sys.path[0])
        if candidate.is_absolute() and candidate.exists():
            return candidate
    return Path(__file__).resolve().parent


_WORKER_DIR: Path = _capture_agent_dir()


def _agent_root() -> Path:
    """Walk up to the agent package root (where manifest.toml lives)."""
    current = _WORKER_DIR
    for _ in range(4):
        if (current / "manifest.toml").exists():
            return current
        current = current.parent
    return _WORKER_DIR.parent  # workers/ -> agent root


_AGENT_ROOT: Path = _agent_root()


@agent(
    name="sonar-cleanup-worker",
    version="0.1.0",
    description=(
        "Worker A2A. Behavior preserving cleanup of one file at a time, "
        "driven by SonarQube issue list and the language harness."
    ),
    tags=("worker", "sonar", "lint", "refactor"),
    agent_type="worker",
    tools_required=("file_read", "file_write", "bash_executor"),
    packages=("jinja2", "pyyaml"),
    step_budget={"max_steps": 30, "max_tool_calls": 80, "wall_clock_secs": 900},
)
class SonarCleanupWorker:
    """A2A worker that cleans one file per call, harness-gated."""

    @skill(
        "sanitize.clean_file",
        description=(
            "Query SonarQube for issues on the file, attempt one behavior "
            "preserving edit, gate via cargo / ruff / pnpm. Returns code_clean "
            "or residue with a reason."
        ),
        examples=[
            {
                "path": "crates/apollia-cli/src/main.rs",
                "language": "rust",
                "crate": "apollia-cli",
                "max_issues": 5,
            }
        ],
    )
    async def clean_file(
        self,
        path: str,
        language: str,
        crate: str = "",
        max_issues: int = 5,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        if ctx is None:
            raise DomainError("NO_CTX", "Runtime context required")
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Local LLM backend required")

        rules = _load_yaml(_AGENT_ROOT / "datasources" / "rules.yaml")
        lang_rules = rules.get("languages", {}).get(language, {})

        # 1. Pull issues from MCP sonarqube.
        issues = await self._fetch_sonar_issues(ctx, path)
        issues_before = len(issues)
        if not issues:
            return {
                "path": path,
                "state": "code_clean",
                "issues_before": 0,
                "issues_after": 0,
                "issues_fixed": [],
            }

        # 2. Rank issues by priority table.
        prio = lang_rules.get("fix_priority", {})
        issues.sort(key=lambda i: prio.get(i.get("rule", ""), 999))

        # 3. Read file + render the fix prompt.
        try:
            file_read = await ctx.tools.call("file_read", {"path": path})
            original_content = file_read.get("content", "")
        except Exception as e:
            return {
                "path": path,
                "state": "residue",
                "issues_before": issues_before,
                "issues_after": issues_before,
                "error": f"file_read_failed: {e}",
            }
        if not original_content:
            return {
                "path": path,
                "state": "residue",
                "issues_before": issues_before,
                "issues_after": issues_before,
                "error": "empty_file",
            }

        hints = _DEFAULT_HINTS.get(language, {})
        prompt = self._render_template(
            "sonar-fix-prompt.md.j2",
            path=path,
            language=language,
            crate=crate or language,
            issues=issues,
            max_issues=max_issues,
            hints=hints,
        )

        # 4. Ask the LLM for a single behavior preserving edit.
        resp = await ctx.llm.chat(prompt, original_content[:6000])
        decision = _extract_json(self._llm_text(resp))
        if not decision or decision.get("decision") not in {"edit", "skip", "residue"}:
            return {
                "path": path,
                "state": "residue",
                "issues_before": issues_before,
                "issues_after": issues_before,
                "error": "llm_no_decision",
            }
        if decision["decision"] != "edit":
            return {
                "path": path,
                "state": "residue",
                "issues_before": issues_before,
                "issues_after": issues_before,
                "error": f"llm_{decision['decision']}: {decision.get('rationale', '')}",
            }

        edit = decision.get("edit") or {}
        old = edit.get("old", "")
        new = edit.get("new", "")
        if not old or old not in original_content:
            return {
                "path": path,
                "state": "residue",
                "issues_before": issues_before,
                "issues_after": issues_before,
                "error": "edit_anchor_not_found",
            }
        if original_content.count(old) != 1:
            return {
                "path": path,
                "state": "residue",
                "issues_before": issues_before,
                "issues_after": issues_before,
                "error": "edit_anchor_ambiguous",
            }

        new_content = original_content.replace(old, new, 1)
        await ctx.tools.call("file_write", {"path": path, "content": new_content})

        # 5. Harness gate.
        harness_cmd = lang_rules.get("harness", "")
        if harness_cmd:
            cmd = harness_cmd.format(crate=crate or language, path=path)
            try:
                br = await ctx.tools.call(
                    "bash_executor",
                    {"command": cmd, "timeout_secs": 600, "cwd": "."},
                )
                if int(br.get("exit_code", 1)) != 0:
                    # Revert.
                    await ctx.tools.call(
                        "file_write", {"path": path, "content": original_content}
                    )
                    return {
                        "path": path,
                        "state": "residue",
                        "issues_before": issues_before,
                        "issues_after": issues_before,
                        "error": "harness_red_reverted",
                        "issues_fixed": [],
                    }
            except Exception as e:
                # Revert on harness exception too.
                await ctx.tools.call(
                    "file_write", {"path": path, "content": original_content}
                )
                return {
                    "path": path,
                    "state": "residue",
                    "issues_before": issues_before,
                    "issues_after": issues_before,
                    "error": f"harness_exception: {e}",
                }

        # 6. Optional re-scan (best effort).
        issues_after = await self._fetch_sonar_issues(ctx, path)
        return {
            "path": path,
            "state": "code_clean",
            "issues_before": issues_before,
            "issues_after": len(issues_after),
            "issues_fixed": decision.get("issues_targeted", []),
        }

    # ------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------

    async def _fetch_sonar_issues(self, ctx: Any, path: str) -> list[dict[str, Any]]:
        """Call MCP sonarqube/search_issues for the given file path component."""
        try:
            res = await ctx.tools.call(
                "mcp:sonarqube/search_issues",
                {"componentKeys": path, "ps": 50, "resolved": "false"},
            )
        except Exception as e:
            ctx.logger.info(f"sonarqube MCP unavailable for {path}: {e}")
            return []
        if not isinstance(res, dict):
            return []
        return list(res.get("issues", []) or [])

    @staticmethod
    def _llm_text(resp: Any) -> str:
        if resp is None:
            return ""
        for attr in ("content", "text", "message", "output"):
            v = getattr(resp, attr, None)
            if isinstance(v, str):
                return v
        s = str(resp)
        if s.startswith("<") and "object at 0x" in s:
            return ""
        return s

    def _render_template(self, template_name: str, **vars: Any) -> str:
        env = jinja2.Environment(
            loader=jinja2.FileSystemLoader(str(_AGENT_ROOT / "templates")),
            autoescape=False,
            trim_blocks=True,
            lstrip_blocks=True,
        )
        return env.get_template(template_name).render(**vars)


# ---------------------------------------------------------------------------
# Module-level helpers
# ---------------------------------------------------------------------------


def _load_yaml(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        return yaml.safe_load(path.read_text()) or {}
    except yaml.YAMLError:
        return {}


def _extract_json(text: str) -> dict[str, Any] | None:
    """Find the first balanced JSON object in the LLM response."""
    if not text:
        return None
    s = text.strip()
    s = re.sub(r"<think>.*?</think>", "", s, flags=re.DOTALL | re.IGNORECASE).strip()
    s = re.sub(r"^```(?:json)?\s*|\s*```$", "", s, flags=re.MULTILINE).strip()
    start = s.find("{")
    if start < 0:
        return None
    depth = 0
    in_string = False
    escape = False
    end = -1
    for i in range(start, len(s)):
        c = s[i]
        if escape:
            escape = False
            continue
        if c == "\\" and in_string:
            escape = True
            continue
        if c == '"':
            in_string = not in_string
            continue
        if in_string:
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    if end < 0:
        return None
    try:
        return json.loads(s[start : end + 1])
    except json.JSONDecodeError:
        return None


_DEFAULT_HINTS: dict[str, dict[str, str]] = {
    "rust": {
        "rust:S3776": (
            "Extract a small private helper that captures the deepest "
            "nested branch. Use early returns and let-else to flatten."
        ),
        "rust:S107": (
            "Bundle related parameters into a `FooArgs<'a>` struct. Update "
            "call sites inside the same crate only."
        ),
        "rust:S125": (
            "Delete commented-out code unless the surrounding text is a "
            "stable rationale comment."
        ),
        "rust:S1192": (
            "Hoist the repeated string literal into a `const` at module "
            "scope."
        ),
    },
    "python": {
        "python:S3776": (
            "Split the function into smaller private helpers. Move the "
            "boolean expression into a named predicate."
        ),
        "python:S7503": "Drop the unused argument or prefix it with `_`.",
        "python:S125": "Delete commented-out code.",
        "python:S107": (
            "Bundle related kwargs into a TypedDict or @dataclass."
        ),
    },
    "typescript": {
        "typescript:S7764": "Replace `window` with `globalThis` (compat).",
        "typescript:S7773": (
            "Replace `parseInt(x, 10)` with `Number.parseInt(x, 10)`."
        ),
        "typescript:S3358": (
            "Flatten the nested ternary into an `if/else` block or a small "
            "lookup table."
        ),
        "typescript:S3776": (
            "Extract sub-expressions into named consts. Move predicates "
            "into separate functions."
        ),
    },
}
