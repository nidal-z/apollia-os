"""standup-scribe — Daily standup report automation for software teams.

╔══════════════════════════════════════════════════════════════════════════════╗
║  BUSINESS SCENARIO                                                           ║
╚══════════════════════════════════════════════════════════════════════════════╝

Every weekday morning at 08:30, this agent:

  1. Scans your configured Git repositories for the past 24 hours of commits.
  2. Reads any notes you dropped in NOTES_DIR overnight.
  3. Recalls your past standups from memory for tone and context continuity.
  4. Drafts a structured standup: Yesterday / Today / Blockers.
  5. Asks for your approval (HITL) before writing the report to disk.
  6. Saves the approved report to STANDUP_DIR/YYYY-MM-DD.md.
  7. Stores the standup summary in memory for long-term trend analysis.

You review once — it writes, saves, and keeps your history forever.

╔══════════════════════════════════════════════════════════════════════════════╗
║  APOLLIA FEATURES DEMONSTRATED                                               ║
╚══════════════════════════════════════════════════════════════════════════════╝

  • Mode Direct       LLM-driven ReAct loop with explicit step control
  • Cron trigger      Fires automatically at 08:30 Mon–Fri
  • Memory engine     Recalls past standups; learns your writing style over time
  • HITL approval     You validate the draft before it is written to disk
  • bash_executor     Runs `git log` and reads external files via shell
  • file_io           Writes the approved standup to the sandbox workspace
  • python_executor   Parses and formats raw git output with Python
  • Notifications     Desktop alert when your standup is ready for review

╔══════════════════════════════════════════════════════════════════════════════╗
║  TRIGGER CONFIGURATION  (add to ~/.apollia/apollia.toml)                    ║
╚══════════════════════════════════════════════════════════════════════════════╝

  [[triggers]]
  id       = "standup-morning"
  agent    = "standup-scribe"
  enabled  = true
  on_busy  = "drop"                   # only one standup per day

  [triggers.source]
  type     = "cron"
  schedule = "30 8 * * MON-FRI"      # 08:30 every weekday

  [triggers.input]
  text     = "Generate my daily standup for today."

╔══════════════════════════════════════════════════════════════════════════════╗
║  NOTIFICATION CONFIGURATION  (add to ~/.apollia/apollia.toml)               ║
╚══════════════════════════════════════════════════════════════════════════════╝

  [notifications]
  events = ["task.input_required", "task.completed", "task.failed"]

  [[notifications.channels]]
  id      = "desktop"
  type    = "desktop"
  enabled = true
  events  = ["task.input_required", "task.completed"]

╔══════════════════════════════════════════════════════════════════════════════╗
║  QUICK START                                                                 ║
╚══════════════════════════════════════════════════════════════════════════════╝

  1. Edit GIT_REPOS below to point to your local repositories.
  2. Create the notes folder: mkdir -p ~/standup-notes
  3. Start Apollia and register this agent.
  4. Add the trigger + notification config above to ~/.apollia/apollia.toml
  5. Trigger manually at any time:
       apollia-os run standup-scribe "Generate my daily standup for today."
"""

import json
import os
from datetime import datetime

from apollia_base import AIPResult, BaseReActAgent, resume_pending_tool

# ─────────────────────────────────────────────────────────────────────────────
# Configuration — edit these values to match your environment
# ─────────────────────────────────────────────────────────────────────────────

# Git repositories to scan for recent commits.
# Add all projects you actively work on.
GIT_REPOS: list[str] = [
    os.path.expanduser("~/Documents/apollia-v2"),
]

# Drop folder for quick notes (optional). Create any .txt or .md file here
# and the agent will include its contents in the standup context.
NOTES_DIR: str = os.path.expanduser("~/standup-notes")

# Where the approved standup reports are saved (YYYY-MM-DD.md).
# This path is inside the agent sandbox workspace.
STANDUP_FILENAME_TEMPLATE: str = "{date}-standup.md"

# How many hours back to look in git history.
GIT_LOOKBACK_HOURS: int = 24

# How many past standups to load from memory for context.
MEMORY_RECALL_LIMIT: int = 3


# ─────────────────────────────────────────────────────────────────────────────
# System prompt — the LLM's identity and output contract
# ─────────────────────────────────────────────────────────────────────────────

_SYSTEM_PROMPT = """\
You are Standup Scribe, a professional assistant that generates concise daily \
standup reports for software developers.

Your goal is to produce a clear, accurate, and useful standup in exactly this \
three-section markdown format:

## Yesterday
- [what was actually done, inferred from git commits and notes]

## Today
- [logical next steps based on the open work visible in the commits and notes]

## Blockers
- [explicit blockers found in notes, or "None identified."]

RULES:
- Be specific: reference actual commit messages, branch names, or file names.
- Be concise: each bullet is one sentence maximum.
- Do not invent work that is not evidenced by commits or notes.
- If no commits are found for a repo, mention it explicitly.
- Maintain the same tone and vocabulary as past standups if provided.
- The final output should be ready to paste directly into Slack or Notion.
- When writing the report file, use the exact path provided — do not modify it.
"""


# ─────────────────────────────────────────────────────────────────────────────
# Agent
# ─────────────────────────────────────────────────────────────────────────────

class StandupScribeAgent(BaseReActAgent):
    """Daily standup report automation using git history and personal notes.

    Flow (normal):
      bash_executor × N  → git log for each repo + notes scan
      python_executor × 1 → format raw git output into clean commit list
      LLM reasoning       → draft the three-section standup
      file_io (HITL)      → write approved report → suspension
      [user approves]     → resume → file_io executes → done

    Flow (HITL resume):
      resume_pending_tool → extract deferred file_io call
      react()             → execute the write immediately
    """

    SYSTEM_PROMPT = _SYSTEM_PROMPT
    MAX_STEPS = 14

    def manifest(self) -> dict:
        return {
            "name": "standup-scribe",
            "version": "1.0.0",
            "description": (
                "Generates your daily standup from git history and notes. "
                "Fires every weekday at 08:30. Asks for approval before saving."
            ),
            "tools_required": ["bash_executor"],
            "tools_optional": ["file_io", "python_executor"],
            # HITL: the agent asks for your approval before writing the report.
            "tools_requiring_approval": ["file_io"],
            "memory_namespace": "standup-scribe",
            "execution_mode": "direct",
            "max_concurrent_tasks": 1,
            "step_budget": {
                "max_steps": 20,
                "max_tool_calls": 30,
                "wall_clock_secs": 180,
            },
            "dangerous_tools_allowed": False,
            "tags": ["productivity", "developer-tools", "daily-reporting"],
        }

    async def run(self, task: dict, ctx) -> dict:
        today = datetime.now().strftime("%Y-%m-%d")
        report_filename = STANDUP_FILENAME_TEMPLATE.format(date=today)
        user_input = _extract_text(task)

        # ── HITL resume: execute the deferred file_io write ───────────────────
        if task.get("is_resumed"):
            pending = resume_pending_tool(task)
            result = await self.react(task, ctx, user_input, pending_tool=pending)
            if isinstance(result, dict):
                return result
            return AIPResult.completed(
                f"✓ Standup for {today} saved as `{report_filename}`.\n\n{result}"
            )

        # ── Load past standups from memory for style/context ──────────────────
        past_standups_context = await _load_past_standups(ctx)

        # ── Build task context for the ReAct loop ────────────────────────────
        repos_str = "\n".join(f"  - {r}" for r in GIT_REPOS)
        extra_context = (
            f"Today's date: {today}\n"
            f"Standup report filename (use exactly): {report_filename}\n"
            f"Git repositories to scan:\n{repos_str}\n"
            f"Notes directory to check: {NOTES_DIR}\n"
            f"Git lookback window: last {GIT_LOOKBACK_HOURS} hours\n"
        )
        if past_standups_context:
            extra_context += f"\n{past_standups_context}"

        # ── Run the ReAct loop ────────────────────────────────────────────────
        result = await self.react(
            task,
            ctx,
            user_message=(
                f"Generate the daily standup for {today}. "
                "Step 1: run git log for each repo to get recent commits. "
                "Step 2: check the notes directory for any .txt or .md files. "
                "Step 3: draft the standup in the required format. "
                f"Step 4: write the report to file `{report_filename}` — "
                "this will trigger an approval request."
            ),
            extra_context=extra_context,
        )

        if isinstance(result, dict):
            # AIPResult.input_required (awaiting approval) or AIPResult.failed
            return result

        # ── Persist standup to memory for future context ──────────────────────
        if ctx.memory:
            try:
                await ctx.memory.record(
                    content=f"Standup {today}:\n{result[:600]}",
                    importance=0.8,
                    task_id=task.get("task_id"),
                )
                await ctx.memory.remember(
                    key=f"standup:{today}",
                    value=result,
                    source="standup-scribe",
                )
            except Exception:
                pass  # Memory errors must never crash the agent

        return AIPResult.completed(
            f"✓ Standup for {today} saved as `{report_filename}`.\n\n{result}"
        )


# ─────────────────────────────────────────────────────────────────────────────
# Private helpers
# ─────────────────────────────────────────────────────────────────────────────

def _extract_text(task: dict) -> str:
    """Extract the first text part from a task input."""
    for part in task.get("input", {}).get("parts", []):
        if part.get("type") == "text":
            return part["text"]
    return "Generate my daily standup for today."


async def _load_past_standups(ctx) -> str:
    """Return a formatted string with recent standup snippets from memory."""
    if ctx.memory is None:
        return ""
    try:
        results = await ctx.memory.search("standup report", limit=MEMORY_RECALL_LIMIT)
        if not results:
            return ""
        snippets = [r.get("content", "") for r in results if r.get("content")]
        if not snippets:
            return ""
        joined = "\n---\n".join(s[:300] for s in snippets[:MEMORY_RECALL_LIMIT])
        return (
            "Recent standups for reference (match tone and vocabulary):\n"
            f"{joined}\n"
        )
    except Exception:
        return ""


# ─────────────────────────────────────────────────────────────────────────────
# Module-level agent instance (required by the Apollia AIP contract)
# ─────────────────────────────────────────────────────────────────────────────

agent = StandupScribeAgent()
