"""WorkerAgent — base class for domain-specialized agents.

Provides helper methods for common tool call patterns, reducing
boilerplate in domain agents. Does not change the ReAct loop.
All helpers are thin wrappers around ctx.tools.call().
"""
from __future__ import annotations

from typing import Any

from apollia.agents.react import AIPResult, BaseReActAgent


class WorkerAgent(BaseReActAgent):
    """Base class for domain-specialized Worker Agents.

    Extends BaseReActAgent with helper methods for the most common
    tool call patterns in domain agents. This class is a convention,
    not a runtime requirement — BaseReActAgent is always sufficient.
    """

    # ── Python execution ──────────────────────────────────────────────────

    async def run_python(
        self,
        ctx: Any,
        code: str,
        timeout_secs: int = 30,
    ) -> dict[str, Any]:
        """Execute Python code via python_executor.

        Returns dict with keys: stdout, stderr, exit_code, duration_ms.
        """
        return await ctx.tools.call("python_executor", {
            "code": code,
            "timeout_secs": timeout_secs,
        })

    # ── File operations ───────────────────────────────────────────────────

    async def read_file(self, ctx: Any, path: str) -> str:
        """Read a file and return its content as a string."""
        result = await ctx.tools.call("file_read", {"path": path})
        return result.get("content", "")

    async def write_file(self, ctx: Any, path: str, content: str) -> None:
        """Write content to a file (atomic write).

        Creates parent directories if needed.
        """
        await ctx.tools.call("file_write", {
            "path": path,
            "content": content,
        })

    async def list_files(
        self,
        ctx: Any,
        path: str,
        recursive: bool = False,
    ) -> list[str]:
        """List files in a directory.

        Returns list of relative paths from the directory root.
        """
        result = await ctx.tools.call("file_list", {
            "path": path,
            "recursive": recursive,
        })
        return result.get("entries", [])

    # ── A2A delegation ────────────────────────────────────────────────────

    async def delegate_skill(
        self,
        ctx: Any,
        skill_id: str,
        payload: dict[str, Any],
        timeout_secs: int = 120,
    ) -> dict[str, Any]:
        """Delegate a task to another agent by skill ID via A2A routing.

        Resolves the skill at runtime, submits the task, and waits for
        the result.  Raises ``RuntimeError`` on skill-not-found, ambiguity,
        or timeout.
        """
        return await ctx.delegate(skill_id, payload, timeout_secs)

    # ── Error helpers ──────────────────────────────────────────────────────

    def domain_error(
        self,
        code: str,
        message: str,
        details: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Return a typed domain failure result.

        Stable snake_case error codes:
        ``file_not_found``, ``corrupted_file``, ``parse_error``,
        ``sheet_not_found``, ``column_not_found``, ``encoding_error``,
        ``python_execution_failed``, ``permission_denied``.
        """
        return AIPResult.failed(code, message, details)

    def check_python_result(
        self,
        result: dict[str, Any],
        operation: str,
    ) -> str | dict[str, Any]:
        """Validate python_executor output.

        Returns the stdout string on success (exit_code == 0).
        Returns an ``AIPResult.failed()`` dict on non-zero exit code.
        The caller must check whether the return value is a ``dict``
        (failure) or a ``str`` (success).
        """
        if result.get("exit_code", 1) != 0:
            stderr = result.get("stderr", "unknown error")
            return self.domain_error(
                "python_execution_failed",
                f"{operation} failed: {stderr[:500]}",
                {"stderr": stderr, "exit_code": result.get("exit_code")},
            )
        return result.get("stdout", "")
