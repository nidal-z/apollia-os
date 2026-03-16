"""document-analyst — LLM-powered document analysis agent.

Listens for new files in a watched folder. When a file arrives, the agent
reads it, injects the full content into the LLM context, then lets the model
reason and act: classify the document, extract structured intelligence, write
a Markdown report, and return a one-line summary.

No hardcoded parsing logic — the LLM plans every step.

APOLLIA FEATURES DEMONSTRATED
  • File watch trigger  Fires the instant a file lands in the watched folder
  • Mode Direct        ReAct loop — LLM reasons and calls tools
  • bash_executor      Reads the document content
  • file_io            Writes the structured analysis report
  • Memory engine      Builds a searchable knowledge base over time
"""

import json
import os
from datetime import datetime

from apollia_base import AIPResult, BaseReActAgent

# ─────────────────────────────────────────────────────────────────────────────
# Configuration
# ─────────────────────────────────────────────────────────────────────────────

# Maximum characters forwarded to the LLM context.
MAX_CONTENT_CHARS: int = 6_000

# Report output directory (relative to $HOME — file_io sandbox root).
REPORTS_DIR: str = ".apollia/reports"


# ─────────────────────────────────────────────────────────────────────────────
# Agent
# ─────────────────────────────────────────────────────────────────────────────

class DocumentAnalystAgent(BaseReActAgent):
    """LLM-based document analysis agent using the ReAct loop.

    Execution flow:
      1. run() extracts the file path from the trigger input.
      2. Reads the file content via bash_executor.
      3. Injects document content as extra_context and calls react().
      4. The LLM reasons about the content, extracts intelligence, and
         calls file_io to write the structured analysis report.
      5. Persists a memory entry for future cross-document search.
    """

    MAX_STEPS = 6

    SYSTEM_PROMPT = """\
You are Document Analyst, an expert at transforming any document into \
structured, actionable intelligence.

You will receive a document's content. Your job:

1. CLASSIFY the document:
   Choose exactly one category: Invoice | Contract | Meeting Notes | \
Research | Report | Other

2. EXTRACT structured intelligence:
   - EXECUTIVE SUMMARY: 2–4 sentences capturing the core message
   - KEY ENTITIES: people, organizations, dates, amounts (bullet list)
   - ACTION ITEMS: concrete tasks or follow-ups with owner if present \
(bullet list, or "None identified.")
   - KEY FIGURES: monetary amounts, reference numbers, IBANs, percentages
   - FLAGS: risks, deadlines, penalty clauses, urgent items \
(bullet list, or "None identified.")

3. WRITE the analysis report using file_io:
   - action: "write"
   - path: the report_path given in the user message
   - content: a clean Markdown report following this template exactly:

# Document Analysis: <TITLE>
**Date:** <today>
**Category:** <CATEGORY>

## Executive Summary
<2–4 sentences>

## Key Entities
<bullet list>

## Key Figures
<bullet list>

## Action Items
<bullet list or "None identified.">

## Flags
<bullet list or "None identified.">

4. Return a final_answer with a one-line summary:
   "<CATEGORY>: <title or subject> — <N> action items, <N> flags"

RULES:
- Never fabricate information not present in the document.
- Complete all steps before returning final_answer.
- Use ONLY the tools available to you.
"""

    def manifest(self) -> dict:
        return {
            "name": "document-analyst",
            "version": "2.0.0",
            "description": (
                "LLM-powered document analysis agent. Fires on new files, "
                "extracts structured intelligence, writes a Markdown report."
            ),
            "tools_required": ["bash_executor", "file_io"],
            "tools_optional": [],
            "memory_namespace": "document-analyst",
            "execution_mode": "direct",
            "max_concurrent_tasks": 2,
            "step_budget": {
                "max_steps": 10,
                "max_tool_calls": 8,
                "wall_clock_secs": 120,
            },
            "dangerous_tools_allowed": False,
            "tags": ["productivity", "knowledge-management", "document-processing"],
        }

    async def run(self, task: dict, ctx) -> dict:
        """Entry point called by the Apollia runtime for each task.

        1. Extract the document path from the trigger input.
        2. Read the file content via bash_executor.
        3. Delegate to the LLM ReAct loop.
        4. Record insights to memory.
        """
        doc_path = _extract_document_path(task)
        if not doc_path:
            return AIPResult.failed("NO_PATH", "Could not extract document path from task input.")

        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "DocumentAnalystAgent requires an LLM for ReAct execution (no deterministic fallback).",
            )

        doc_name = os.path.basename(doc_path)
        report_path = f"{REPORTS_DIR}/{os.path.splitext(doc_name)[0]}.analysis.md"
        today = datetime.now().strftime("%Y-%m-%d %H:%M")

        # ── Read document content ─────────────────────────────────────────────
        content = await _read_file(ctx, doc_path)
        if not content:
            return AIPResult.failed("UNREADABLE", f"Could not read: {doc_path}")

        truncated = content[:MAX_CONTENT_CHARS]
        was_truncated = len(content) > MAX_CONTENT_CHARS

        # ── Run ReAct ──────────────────────────────────────────────────────────
        result = await self._run_llm(
            task,
            ctx,
            doc_name,
            doc_path,
            report_path,
            today,
            truncated,
            was_truncated,
        )

        if isinstance(result, dict):
            return result  # AIPResult.failed / AIPResult.input_required

        # ── Persist to memory ─────────────────────────────────────────────────
        if ctx.memory:
            try:
                await ctx.memory.record(
                    content=f"[{today}] Analyzed: {doc_name}. Summary: {result}",
                    importance=0.7,
                    task_id=task.get("task_id"),
                )
                await ctx.memory.remember(
                    key=f"analyzed:{doc_name}",
                    value=json.dumps({"analyzed_at": today, "source_path": doc_path,
                                      "task_id": task.get("task_id", "")}),
                    source="document-analyst",
                )
            except Exception:
                pass

        return AIPResult.completed(result)

    # ── LLM path ──────────────────────────────────────────────────────────────

    async def _run_llm(self, task, ctx, doc_name, doc_path, report_path, today, content, was_truncated):
        """Drive the ReAct loop: inject document content, let LLM plan and act."""
        truncation_note = f"\n[Content truncated at {MAX_CONTENT_CHARS} chars]" if was_truncated else ""
        extra_context = (
            f"=== DOCUMENT: {doc_name} ===\n"
            f"{content}{truncation_note}\n"
            f"=== END DOCUMENT ==="
        )
        user_msg = (
            f"Analyze the document above.\n"
            f"Report path: {report_path}\n"
            f"Today's date: {today}"
        )
        return await self.react(task, ctx, user_msg, extra_context=extra_context)


# ─────────────────────────────────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────────────────────────────────

async def _read_file(ctx, path: str) -> str:
    """Read a file via bash_executor and return its content."""
    if not ctx.tools:
        return ""
    try:
        result = await ctx.tools.call("bash_executor", {
            "command": f"cat '{path}' 2>/dev/null || echo 'FILE_UNREADABLE'",
            "timeout_seconds": 10,
        })
        if isinstance(result, dict):
            stdout = result.get("stdout", "")
        else:
            stdout = str(result)
        if "FILE_UNREADABLE" in stdout[:30]:
            return ""
        return stdout
    except Exception:
        return ""


def _extract_document_path(task: dict) -> str:
    """Extract the document path from the trigger input text."""
    for part in task.get("input", {}).get("parts", []):
        if part.get("type") != "text":
            continue
        text = part["text"]
        for prefix in ("Analyze the new document:", "Analyze the new file:", "Analyze:"):
            if prefix in text:
                return text.split(prefix, 1)[1].strip()
        # Fallback: last token that looks like a path
        for token in reversed(text.split()):
            if token.startswith("/") or token.startswith("~"):
                return os.path.expanduser(token)
    return ""


# ─────────────────────────────────────────────────────────────────────────────
# Module-level agent instance (required by the Apollia AIP contract)
# ─────────────────────────────────────────────────────────────────────────────

agent = DocumentAnalystAgent()
