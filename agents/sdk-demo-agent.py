"""sdk-demo-agent — Demonstration agent showcasing all Apollia SDK features.

Business scenario:
  Analyzes the current project structure and produces a formatted report.
  Demonstrates tool introspection, memory persistence, LLM analysis,
  destructive action detection (HITL), JSON parsing, and Markdown formatting.

Apollia features demonstrated:
  - BaseReActAgent inheritance (apollia.agents)
  - AIPResult factory (apollia.agents)
  - Tool introspection (ctx.tools.list_tools, ctx.tools.describe)
  - Memory persistence (ctx.memory.recall, ctx.memory.record, ctx.memory.remember)
  - HITL suspension (AIPResult.input_required)
  - JSON extraction (apollia.utils.parsing.extract_json)
  - Text truncation (apollia.utils.parsing.truncate)
  - Markdown formatting (apollia.utils.formatting.format_as_markdown)

Quick start:
  apollia-os agent start sdk-demo-agent
  apollia-os run sdk-demo-agent --input "Analyze the project structure"
"""

from __future__ import annotations

import json
from typing import Any

from apollia.agents import AIPResult, BaseReActAgent
from apollia.utils.formatting import format_as_markdown
from apollia.utils.parsing import extract_json, truncate


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DESTRUCTIVE_KEYWORDS: tuple[str, ...] = (
    "delete",
    "remove",
    "drop",
    "destroy",
    "rm -rf",
    "rmdir",
    "truncate table",
)

OBSERVATION_MAX_CHARS: int = 1500

MEMORY_KEY_LAST_ANALYSIS: str = "sdk_demo:last_analysis"

_SYSTEM_PROMPT = """\
You are Project Analyst, a helpful assistant that analyzes project structures \
and produces concise, structured reports.

When given a directory listing or project information, respond with a JSON \
object containing:

{
  "summary": "One-paragraph overview of the project",
  "highlights": ["notable observation 1", "notable observation 2"],
  "file_count": <number of files or "unknown">,
  "suggestion": "One actionable improvement suggestion"
}

Rules:
- Be factual — only report what the data shows.
- Keep the summary under 3 sentences.
- Return valid JSON only, no surrounding text.
"""


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

class SdkDemoAgent(BaseReActAgent):
    """Project analysis agent demonstrating the full Apollia SDK feature set.

    Execution flow:
      1. Introspect available tools via ctx.tools.
      2. Recall previous analysis from memory.
      3. Guard against destructive requests (HITL).
      4. Gather project data via bash_executor.
      5. Analyze with LLM and parse the JSON response.
      6. Persist results to memory.
      7. Format and return a Markdown report.
    """

    SYSTEM_PROMPT = _SYSTEM_PROMPT
    MAX_STEPS = 15

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest."""
        return {
            "name": "sdk-demo-agent",
            "version": "1.0.0",
            "description": (
                "Demonstration agent showcasing all Apollia SDK features: "
                "tool introspection, memory, HITL, parsing, and formatting."
            ),
            "tools_required": ["bash_executor"],
            "tools_optional": ["file_io"],
            "tools_requiring_approval": ["file_io"],
            "memory_namespace": "sdk-demo-agent",
            "execution_mode": "direct",
            "max_concurrent_tasks": 1,
            "step_budget": {
                "max_steps": 20,
                "max_tool_calls": 15,
                "wall_clock_secs": 120,
            },
            "dangerous_tools_allowed": False,
            "tags": ["demo", "sdk", "project-analysis"],
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute a multi-step workflow demonstrating SDK features.

        Args:
            task: AIP task dict from the runtime.
            ctx: RuntimeContext with ctx.llm, ctx.tools, ctx.memory.

        Returns:
            AIPResult dict (completed, failed, or input_required).
        """
        input_text = _extract_text(task)
        is_resumed = task.get("is_resumed", False)

        # Step 1: Tool introspection
        available_tools = ctx.tools.list_tools() if ctx.tools else []
        tool_info = await _introspect_tools(ctx, available_tools)

        # Step 2: Memory — recall previous analysis context
        previous_context = await _recall_previous(ctx)

        # Step 3: HITL — guard destructive requests (skip on resume)
        if not is_resumed and _is_destructive(input_text):
            return AIPResult.input_required(
                f"Action appears destructive: "
                f"'{truncate(input_text, 100)}'. Confirm?",
                {"original_input": input_text},
            )

        # Step 4: Gather data via tool call
        observation = await _gather_project_data(ctx, available_tools)

        # Step 5: LLM analysis + JSON extraction
        analysis = await _analyze_with_llm(ctx, input_text, observation)

        # Step 6: Persist to memory
        await _persist_to_memory(ctx, task, analysis)

        # Step 7: Format as Markdown report
        highlights = analysis.get("highlights", [])
        report_data = {
            "Request": input_text,
            "Tools Available": ", ".join(available_tools) or "None",
            "Tool Details": tool_info or "N/A",
            "Previous Context": previous_context or "First run",
            "Analysis": analysis.get("summary", "No analysis available"),
            "Highlights": "; ".join(highlights) if highlights else "None",
            "Suggestion": analysis.get("suggestion", "N/A"),
        }
        output = format_as_markdown(report_data)

        return AIPResult.completed(output)


# ---------------------------------------------------------------------------
# Private helpers
# ---------------------------------------------------------------------------

def _extract_text(task: dict[str, Any]) -> str:
    """Extract the first text part from a task input."""
    for part in task.get("input", {}).get("parts", []):
        if part.get("type") == "text":
            return part["text"]
    return "Analyze the project structure"


def _is_destructive(text: str) -> bool:
    """Return True if the input text contains destructive action keywords."""
    lower = text.lower()
    return any(kw in lower for kw in DESTRUCTIVE_KEYWORDS)


def _llm_content(response: object) -> str:
    """Extract text content from an LLM response (runtime object or mock dict)."""
    if isinstance(response, dict):
        return str(response.get("content", ""))
    return str(getattr(response, "content", ""))


async def _introspect_tools(ctx: Any, available_tools: list[str]) -> str:
    """Describe the first available tool for demonstration."""
    if not ctx.tools or not available_tools:
        return ""
    target = (
        "bash_executor" if "bash_executor" in available_tools else available_tools[0]
    )
    desc = await ctx.tools.describe(target)
    if desc is None:
        return ""
    return f"{target}: {desc.get('description', 'no description')}"


async def _recall_previous(ctx: Any) -> str | None:
    """Recall the last analysis from memory, if available."""
    if ctx.memory is None:
        return None
    try:
        return await ctx.memory.recall(MEMORY_KEY_LAST_ANALYSIS)
    except Exception as e:
        await ctx.log(f"memory recall failed: {e}")
        return None


async def _gather_project_data(
    ctx: Any,
    available_tools: list[str],
) -> str:
    """Run bash_executor to list the project structure."""
    if not ctx.tools or "bash_executor" not in available_tools:
        return ""
    try:
        result = await ctx.tools.call(
            "bash_executor",
            {"command": "ls -la", "timeout_seconds": 10},
        )
        return truncate(str(result), OBSERVATION_MAX_CHARS)
    except Exception as e:
        await ctx.log(f"project data gathering failed: {e}")
        return ""


async def _analyze_with_llm(
    ctx: Any,
    input_text: str,
    observation: str,
) -> dict[str, Any]:
    """Call the LLM to analyze gathered data and parse the JSON response."""
    if ctx.llm is None:
        return {"summary": "LLM not available — skipped analysis"}
    try:
        response = await ctx.llm.chat(
            system=_SYSTEM_PROMPT,
            user=(
                f"User request: {input_text}\n\n"
                f"Project listing:\n{observation}"
            ),
        )
        content = _llm_content(response)
        parsed = extract_json(content)
        if parsed:
            return parsed
        return {"summary": content}
    except Exception as exc:
        await ctx.log(f"LLM analysis failed: {exc}")
        return {"summary": f"Analysis failed: {exc}"}


async def _persist_to_memory(
    ctx: Any,
    task: dict[str, Any],
    analysis: dict[str, Any],
) -> None:
    """Record the analysis result to episodic and semantic memory."""
    if ctx.memory is None:
        return
    summary = analysis.get("summary", "")
    try:
        await ctx.memory.record(
            content=f"SDK demo analysis: {truncate(summary, 300)}",
            importance=0.7,
            task_id=task.get("task_id"),
        )
        await ctx.memory.remember(
            key=MEMORY_KEY_LAST_ANALYSIS,
            value=json.dumps(analysis),
            source="sdk-demo-agent",
        )
    except Exception as e:
        await ctx.log(f"memory persistence failed: {e}")


# ---------------------------------------------------------------------------
# Module-level agent instance (required by the Apollia AIP contract)
# ---------------------------------------------------------------------------

agent = SdkDemoAgent()
