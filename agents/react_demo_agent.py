"""react_demo_agent — Codebase Explorer demonstrating the ReAct loop.

This agent is the canonical example of the Reason-Act-Observe pattern on
Apollia OS. It explores a local directory, reads key files, and produces a
structured report. Writing the report to disk requires HITL approval.

## Quick start

    apollia-os agent start agents/react_demo_agent.py
    apollia-os run codebase-explorer "/path/to/your/project"

With report writing (triggers HITL):

    apollia-os run codebase-explorer "/path/to/project --write-report"

## What the agent does

1. THINK  — What should I look at first in this project?
2. ACT    — bash_executor: ls -la, find Cargo.toml/package.json/pyproject.toml
3. OBSERVE — directory listing, project type detected
4. THINK  — I should read the README and the main config file.
5. ACT    — file_io: read README.md
6. OBSERVE — README content
7. THINK  — I should check the language breakdown.
8. ACT    — bash_executor: find . -name '*.rs' | wc -l
9. OBSERVE — file count
   ... (continues until enough information is gathered)
N. FINAL  — Produces a structured Markdown report.

If --write-report is in the input, step N+1 requests HITL approval before
writing the report to <directory>/.apollia/report.md.

## HITL flow

    1. Agent returns AIPResult.input_required("Write report to ...?", context)
    2. Runtime suspends the task.
    3. Human approves via: apollia-os task resume <task-id> --approve
       or rejects via:     apollia-os task resume <task-id> --reject
    4. If approved: agent writes the file and returns AIPResult.completed(...)
    5. If rejected: agent returns AIPResult.failed("HITL_REJECTED", ...)
"""

import os

from apollia_base import BaseReActAgent, resume_pending_tool


# ─────────────────────────────────────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────────────────────────────────────

_REPORT_FILENAME: str = "report.md"
_REPORT_DIR: str = ".apollia"

# Flag appended to the user input to request report writing.
_WRITE_REPORT_FLAG: str = "--write-report"

_AGENT_NAME: str = "codebase-explorer"
_AGENT_VERSION: str = "1.0.0"


# ─────────────────────────────────────────────────────────────────────────────
# CodebaseExplorer
# ─────────────────────────────────────────────────────────────────────────────

class CodebaseExplorer(BaseReActAgent):
    """ReAct agent that explores a local codebase and produces a summary report.

    Demonstrates:
      - Multi-step Reason-Act-Observe loop
      - Use of bash_executor and file_io tools
      - HITL approval before writing files (destructive action)
      - Memory recording of produced reports
      - Graceful degradation when ctx.llm or ctx.tools is None
    """

    SYSTEM_PROMPT = (
        "You are a software engineer analysing a local codebase. "
        "Your goal is to explore the directory provided by the user, understand "
        "the project structure, identify the technology stack, find the main entry "
        "point, count source files by language, and summarise what the project does. "
        "Be methodical: start broad (list root, detect project type), then go deep "
        "(read key files like README, Cargo.toml, package.json, pyproject.toml, "
        "main.rs, index.ts, etc.). "
        "Stop exploring when you have enough information. "
        "Your final answer must be a well-structured Markdown report with these "
        "sections: Overview, Technology Stack, Key Files, Source Statistics, "
        "and Summary."
    )

    MAX_STEPS = 12

    def manifest(self) -> dict:
        return {
            "name": _AGENT_NAME,
            "version": _AGENT_VERSION,
            "description": (
                "Explores a local project directory and produces a structured "
                "Markdown report. Requires HITL approval before writing the "
                "report to disk."
            ),
            "tools_required": ["bash_executor", "file_io"],
            # file_io (write) requires human approval — read/list are safe.
            # The agent checks this flag itself before writing (see _write_report).
            "tools_requiring_approval": [],
            "memory_namespace": _AGENT_NAME,
            "execution_mode": "direct",
            "dangerous_tools_allowed": False,
        }

    async def run(self, task: dict, ctx) -> dict:
        """Entry point called once per task by the Apollia runtime.

        Parses the input, handles the HITL write-report flow, and runs the
        ReAct loop.
        """
        # ── 1. Parse input ────────────────────────────────────────────────────
        parts = task.get("input", {}).get("parts", [])
        if not parts or not parts[0].get("text", "").strip():
            return AIPResult.failed(
                "MISSING_INPUT",
                "Provide a directory path as the task input, e.g. /home/user/myproject",
            )

        raw_input = parts[0]["text"].strip()
        write_report = _WRITE_REPORT_FLAG in raw_input
        directory = raw_input.replace(_WRITE_REPORT_FLAG, "").strip()

        if not directory:
            return AIPResult.failed(
                "MISSING_PATH",
                "No directory path found in the input.",
            )

        # ── 2. HITL resume: writing the report to disk ───────────────────────
        # Must be checked before the LLM-availability check because writing
        # the report does not require LLM — only file_io.
        if task.get("is_resumed"):
            return await self._handle_write_resume(task, ctx, directory)

        # ── 3. Fast path: static-only mode when no LLM is configured ─────────
        if ctx.llm is None:
            return await self._static_summary(task, ctx, directory)

        # ── 4. Run the ReAct exploration loop ─────────────────────────────────
        user_message = (
            f"Analyse the project at: {directory}\n\n"
            "Explore the directory, read key files, and produce a comprehensive "
            "Markdown report."
        )

        result = await self.react(task, ctx, user_message)

        # result is either a str (the report) or a dict (AIPResult).
        if isinstance(result, dict):
            return result

        report = result

        # ── 5. Optionally write the report to disk (requires HITL) ────────────
        if write_report:
            report_path = os.path.join(directory, _REPORT_DIR, _REPORT_FILENAME)
            write_result = await self._request_write_approval(
                task, ctx, report, report_path
            )
            if write_result is not None:
                return write_result  # suspended or failed

        # ── 6. Record in memory ───────────────────────────────────────────────
        await self._record_in_memory(task, ctx, directory, report)

        return AIPResult.completed(report)

    # ── HITL helpers ──────────────────────────────────────────────────────────

    async def _request_write_approval(
        self,
        task: dict,
        ctx,
        report: str,
        report_path: str,
    ):
        """Suspend the task to ask for human approval before writing.

        Returns None if approval is not needed (no --write-report flag or
        ctx.tools is unavailable). Returns an AIPResult dict when suspending.
        """
        if ctx.tools is None:
            return None

        return AIPResult.input_required(
            (
                f"The agent wants to write the report to:\n  {report_path}\n\n"
                "Approve to create or overwrite this file."
            ),
            {
                "write_report": True,
                "report_path": report_path,
                "report_content": report,
            },
        )

    async def _handle_write_resume(self, task: dict, ctx, directory: str) -> dict:
        """Handle the HITL resume after the write-report approval request.

        If approved, writes the file and returns AIPResult.completed.
        If rejected, returns AIPResult.failed.
        """
        ir = task.get("input_response")
        if ir is None:
            return AIPResult.failed("RESUME_ERROR", "No input_response on resumed task.")

        if not ir.approved:
            reason = getattr(ir, "reason", None) or "No reason given."
            return AIPResult.failed("HITL_REJECTED", f"Report write rejected: {reason}")

        context = getattr(ir, "context", {}) or {}
        report_path = context.get("report_path", "")
        report_content = context.get("report_content", "")

        if not report_path or not report_content:
            return AIPResult.failed(
                "RESUME_CONTEXT_ERROR",
                "Missing report_path or report_content in HITL context.",
            )

        # Write the report via file_io (audited).
        observation = await self._call_tool(ctx, "bash_executor", {
            "command": f"mkdir -p '{os.path.dirname(report_path)}'",
            "timeout_seconds": 5,
        })

        write_result = await self._call_tool(ctx, "file_io", {
            "action": "write",
            "path": report_path,
            "content": report_content,
        })

        if "error" in write_result.lower() or "Error" in write_result:
            return AIPResult.failed(
                "WRITE_FAILED",
                f"Failed to write report to {report_path}: {write_result}",
            )

        await self._record_in_memory(task, ctx, directory, report_content)

        return AIPResult.completed(
            f"Report written to `{report_path}`.\n\n{report_content}"
        )

    # ── Fallback: no LLM ─────────────────────────────────────────────────────

    async def _static_summary(self, task: dict, ctx, directory: str) -> dict:
        """Produce a minimal summary using only shell commands (no LLM).

        Active when ctx.llm is None — follows the Apollia degradation principle:
        the agent decides whether the absence of LLM is fatal. Here it is not.
        """
        if ctx.tools is None:
            return AIPResult.failed(
                "NO_TOOLS",
                "Neither ctx.llm nor ctx.tools is available — cannot analyse.",
            )

        lines = ["# Codebase Overview (static — no LLM)", ""]

        # Directory listing.
        listing = await self._call_tool(ctx, "bash_executor", {
            "command": f"ls -la '{directory}'",
            "timeout_seconds": 10,
        })
        lines += ["## Root listing", "", "```", listing.strip(), "```", ""]

        # Language file counts.
        for ext, label in [(".rs", "Rust"), (".py", "Python"), (".ts", "TypeScript")]:
            count_str = await self._call_tool(ctx, "bash_executor", {
                "command": (
                    f"find '{directory}' -name '*{ext}' "
                    f"-not -path '*/target/*' -not -path '*/.git/*' | wc -l"
                ),
                "timeout_seconds": 15,
            })
            count = count_str.strip()
            lines.append(f"- {label} files: {count}")

        lines.append("")
        lines.append("*Generated by codebase-explorer (static mode — no LLM configured)*")

        report = "\n".join(lines)
        await self._record_in_memory(task, ctx, directory, report)
        return AIPResult.completed(report)

    # ── Memory ────────────────────────────────────────────────────────────────

    async def _record_in_memory(
        self,
        task: dict,
        ctx,
        directory: str,
        report: str,
    ) -> None:
        """Record the produced report in agent memory (best-effort, never raises)."""
        if ctx.memory is None:
            return
        try:
            await ctx.memory.record(
                f"codebase-report: {directory}",
                importance=0.8,
                task_id=task.get("task_id", "unknown"),
                metadata={"directory": directory, "report_length": len(report)},
            )
        except Exception:
            pass


# ─────────────────────────────────────────────────────────────────────────────
# Module-level agent instance (required by the AIP duck-typing contract)
# ─────────────────────────────────────────────────────────────────────────────

agent = CodebaseExplorer()
