"""apollia_base — Abstract base class for ReAct agents on Apollia OS.

Implements the Reason-Act-Observe loop using ctx.llm and ctx.tools.
Subclasses define a SYSTEM_PROMPT and implement manifest() + run().

## ReAct loop

    for step in range(MAX_STEPS):
        response = await ctx.llm.complete(messages)   # REASON
        action = parse_json_action(response.content)

        if action is final_answer:
            return action["text"]

        if tool requires HITL approval:
            return AIPResult.input_required(...)      # suspend

        observation = await ctx.tools.call(...)        # ACT
        messages.append(observation)                   # OBSERVE

The LLM must output a JSON object on every turn (see OUTPUT FORMAT section
of the system prompt). Unstructured text is treated as a final answer.

## HITL support

Tools listed in manifest()["tools_requiring_approval"] trigger a suspension.
On resume (task["is_resumed"] = True), the agent reads task["input_response"]
and either executes the deferred tool or aborts.

If ctx.memory is available, the conversation history is persisted across
suspension so the loop continues exactly where it stopped.

## Graceful degradation

- ctx.llm is None  → AIPResult.failed("NO_LLM", ...)
- ctx.tools is None → loop still works; all tool calls return an error observation
- ctx.memory is None → HITL resumes from a fresh loop (history is lost)
"""

import json
import re
from abc import ABC, abstractmethod


# ─────────────────────────────────────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────────────────────────────────────

# Maximum iterations enforced at the Python level.
# The Rust StepBudget is the authoritative hard limit.
DEFAULT_MAX_STEPS: int = 10

# Observations longer than this are truncated before being fed back to the LLM.
OBSERVATION_TRUNCATE_CHARS: int = 2000

# Memory importance score for conversation history snapshots.
MEMORY_IMPORTANCE_HISTORY: float = 0.7

# Action type constants matching the LLM output format.
ACTION_TOOL_CALL: str = "tool_call"
ACTION_FINAL_ANSWER: str = "final_answer"

# Memory key prefix used to store conversation history for HITL resume.
_HISTORY_MEMORY_KEY_PREFIX: str = "react_history:"


# ─────────────────────────────────────────────────────────────────────────────
# AIPResult — helper that builds the dicts expected by the Rust AIP bridge
# ─────────────────────────────────────────────────────────────────────────────

class AIPResult:
    """Factory for the result dicts returned by agent run() methods.

    The Rust bridge deserialises the dict into apollia_core::AIPResult.
    All fields are optional on the Rust side (serde default), so only the
    relevant fields need to be set per status.

    Usage:
        return AIPResult.completed("Final answer text")
        return AIPResult.failed("ERROR_CODE", "Human-readable message")
        return AIPResult.input_required("Approve X?", {"tool": "bash", "args": {}})
    """

    @staticmethod
    def completed(text: str) -> dict:
        """Return a successful result with a plain-text output."""
        return {
            "status": "completed",
            "output": [{"type": "text", "text": str(text)}],
        }

    @staticmethod
    def failed(code: str, message: str, details: dict = None) -> dict:
        """Return a failed result with an error code and message."""
        error: dict = {"code": str(code), "message": str(message)}
        if details is not None:
            error["details"] = details
        return {
            "status": "failed",
            "error": error,
        }

    @staticmethod
    def input_required(prompt: str, context: dict = None) -> dict:
        """Suspend the task and request human approval (HITL).

        The runtime delivers the approval decision back via
        task["is_resumed"] and task["input_response"] on the next call.
        """
        return {
            "status": "input_required",
            "input_required_data": {
                "prompt": str(prompt),
                "context": context or {},
            },
        }


# ─────────────────────────────────────────────────────────────────────────────
# Native tool catalogue
# ─────────────────────────────────────────────────────────────────────────────

# Schema descriptions for native tools — injected into the system prompt so the
# LLM knows parameter names and types. Extend when adding new native tools to
# apollia-tools.
_NATIVE_TOOL_SCHEMAS: dict[str, dict] = {
    "bash_executor": {
        "description": "Execute a shell command and return stdout + stderr.",
        "parameters": {
            "command": "str — the shell command to run",
            "timeout_seconds": "int (optional, default 30)",
        },
        "example": '{"command": "ls -la /tmp", "timeout_seconds": 10}',
    },
    "file_io": {
        "description": (
            "Read, write, list, or check existence of files on the local "
            "filesystem."
        ),
        "parameters": {
            "action": "str — one of: read | write | list | exists",
            "path": "str — absolute or relative path",
            "content": "str (write only) — content to write",
            "pattern": "str (list only) — glob pattern, e.g. '*.rs'",
        },
        "example": '{"action": "read", "path": "/tmp/notes.txt"}',
    },
    "python_executor": {
        "description": "Execute Python 3 code in an isolated venv and return stdout.",
        "parameters": {
            "code": "str — the Python source code to run",
            "timeout_seconds": "int (optional, default 30)",
        },
        "example": '{"code": "import sys; print(sys.version)"}',
    },
}


def _describe_tool(tool_name: str) -> str:
    """Return a compact schema string for one tool."""
    schema = _NATIVE_TOOL_SCHEMAS.get(tool_name)
    if schema is None:
        return f"  {tool_name}: (no schema available — use empty args {{}} to probe)"

    params = "\n".join(
        f"      {k}: {v}" for k, v in schema["parameters"].items()
    )
    return (
        f"  {tool_name}:\n"
        f"    Description: {schema['description']}\n"
        f"    Parameters:\n{params}\n"
        f"    Example args: {schema['example']}"
    )


def _build_tools_block(tool_names: list) -> str:
    """Build the tools section of the system prompt."""
    if not tool_names:
        return "No tools are available — provide a final answer directly."
    return "Available tools:\n\n" + "\n\n".join(
        _describe_tool(name) for name in tool_names
    )


# ─────────────────────────────────────────────────────────────────────────────
# Action parsing
# ─────────────────────────────────────────────────────────────────────────────

# Matches a JSON object inside an optional ```json ... ``` fence.
_JSON_FENCE_RE = re.compile(r"```(?:json)?\s*(\{.*?\})\s*```", re.DOTALL)


class ActionParseError(Exception):
    """Raised when the LLM response cannot be parsed as a ReAct action."""


def _extract_json(content: str) -> dict:
    """Extract the first JSON object from an LLM response.

    Tries three strategies in order:
      1. Parse the full content as JSON.
      2. Extract a ```json ... ``` fenced block.
      3. Find the outermost { ... } span.

    Raises ActionParseError when all strategies fail.
    """
    text = content.strip()

    # Strategy 1 — full content is already valid JSON.
    try:
        result = json.loads(text)
        if isinstance(result, dict):
            return result
    except (json.JSONDecodeError, ValueError):
        pass

    # Strategy 2 — JSON is inside a fenced block.
    match = _JSON_FENCE_RE.search(text)
    if match:
        try:
            result = json.loads(match.group(1))
            if isinstance(result, dict):
                return result
        except (json.JSONDecodeError, ValueError):
            pass

    # Strategy 3 — find outermost braces.
    start = text.find("{")
    end = text.rfind("}")
    if start != -1 and end > start:
        try:
            result = json.loads(text[start : end + 1])
            if isinstance(result, dict):
                return result
        except (json.JSONDecodeError, ValueError):
            pass

    raise ActionParseError(
        f"Could not extract a JSON action from: {text[:200]!r}"
    )


def _validate_action(data: dict) -> dict:
    """Validate the structure of a parsed action dict.

    Expected shapes:
      {"thought": "...", "action": "tool_call",    "tool": "name", "args": {...}}
      {"thought": "...", "action": "final_answer", "text": "..."}

    Raises ActionParseError on structural violations.
    """
    action_type = data.get("action")
    if action_type not in (ACTION_TOOL_CALL, ACTION_FINAL_ANSWER):
        raise ActionParseError(
            f"'action' must be '{ACTION_TOOL_CALL}' or '{ACTION_FINAL_ANSWER}', "
            f"got {action_type!r}"
        )

    if action_type == ACTION_TOOL_CALL and "tool" not in data:
        raise ActionParseError(
            "action 'tool_call' requires a 'tool' key"
        )

    if action_type == ACTION_FINAL_ANSWER and "text" not in data:
        raise ActionParseError(
            "action 'final_answer' requires a 'text' key"
        )

    data.setdefault("args", {})
    data.setdefault("thought", "")
    return data


# ─────────────────────────────────────────────────────────────────────────────
# BaseReActAgent
# ─────────────────────────────────────────────────────────────────────────────

class BaseReActAgent(ABC):
    """Abstract base class for ReAct agents running on Apollia OS (Mode Direct).

    ## Subclassing

    Override SYSTEM_PROMPT and implement manifest() and run():

        class MyAgent(BaseReActAgent):
            SYSTEM_PROMPT = "You are a helpful code reviewer."
            MAX_STEPS = 8

            def manifest(self):
                return {
                    "name": "my-agent",
                    "version": "1.0.0",
                    "description": "...",
                    "tools_required": ["bash_executor", "file_io"],
                    "execution_mode": "direct",
                    "dangerous_tools_allowed": False,
                }

            async def run(self, task, ctx):
                user_msg = task["input"]["parts"][0]["text"]

                # Handle HITL resume before starting the loop.
                pending = _resume_pending_tool(task)
                result = await self.react(task, ctx, user_msg, pending_tool=pending)

                if isinstance(result, dict):
                    return result  # AIPResult.input_required / AIPResult.failed
                return AIPResult.completed(result)

    ## HITL pattern

    When a tool requires approval, react() returns AIPResult.input_required().
    The runtime suspends the task. On resume:
      - task["is_resumed"] is True
      - task["input_response"].approved tells you the decision
      - task["input_response"].context holds {"tool": ..., "args": ...}

    Call _resume_pending_tool(task) to extract the pending tool, then pass it
    to react() via pending_tool=. The base class handles the rest.
    """

    SYSTEM_PROMPT: str = "You are a helpful assistant."
    MAX_STEPS: int = DEFAULT_MAX_STEPS

    @abstractmethod
    def manifest(self) -> dict:
        """Return the AIP agent manifest dict."""

    @abstractmethod
    async def run(self, task: dict, ctx) -> dict:
        """Main entry point called once per task by the Apollia runtime."""

    async def react(
        self,
        task: dict,
        ctx,
        user_message: str,
        *,
        extra_context: str = "",
        pending_tool: dict = None,
    ):
        """Run the ReAct loop and return the final answer or an AIPResult dict.

        Returns:
            str  — final answer text on success.
            dict — AIPResult dict (input_required or failed) to be returned
                   directly from run().

        Args:
            task:          AIP task dict from the runtime.
            ctx:           RuntimeContext with ctx.llm, ctx.tools, ctx.memory.
            user_message:  The user's request.
            extra_context: Additional context prepended to user_message (optional).
            pending_tool:  {"tool": str, "args": dict} from a HITL resume (optional).
        """
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "No LLM backend configured — cannot run the ReAct loop.",
            )

        available_tools = ctx.tools.list_tools() if ctx.tools else []
        tools_requiring_approval = set(
            self.manifest().get("tools_requiring_approval", [])
        )

        # Try to restore conversation history if this is a HITL resume.
        messages = await self._load_history(task, ctx)
        if not messages:
            messages = self._initial_messages(
                user_message, available_tools, extra_context
            )

        # If resuming, execute the deferred tool first before entering the loop.
        if pending_tool and task.get("is_resumed"):
            ir = task.get("input_response")
            if ir is not None and not ir.approved:
                reason = getattr(ir, "reason", None) or "No reason given."
                return AIPResult.failed("HITL_REJECTED", reason)

            observation = await self._call_tool(
                ctx, pending_tool["tool"], pending_tool.get("args", {})
            )
            messages.append({
                "role": "assistant",
                "content": (
                    f'{{"thought": "Resuming after approval", '
                    f'"action": "tool_call", '
                    f'"tool": "{pending_tool["tool"]}", '
                    f'"args": {json.dumps(pending_tool.get("args", {}))}}}'
                ),
            })
            messages.append({"role": "tool", "content": observation})

        # ── Main ReAct loop ──────────────────────────────────────────────────
        for step in range(self.MAX_STEPS):
            response = await ctx.llm.complete(messages)

            try:
                action = _validate_action(_extract_json(response.content))
            except ActionParseError:
                # Unstructured response — treat as final answer.
                return response.content

            if action["action"] == ACTION_FINAL_ANSWER:
                return action["text"]

            # ── Tool call branch ─────────────────────────────────────────────
            tool_name = action["tool"]
            tool_args = action.get("args", {})
            thought = action.get("thought", "")

            if tool_name not in available_tools:
                observation = (
                    f"Error: tool '{tool_name}' is not in your allowed list. "
                    f"Allowed tools: {available_tools}"
                )
            elif tool_name in tools_requiring_approval:
                # Persist history so we can resume the loop after approval.
                await self._save_history(task, ctx, messages)
                return AIPResult.input_required(
                    (
                        f"Agent (step {step + 1}) wants to call '{tool_name}'.\n"
                        f"Reasoning: {thought}\n"
                        f"Arguments: {json.dumps(tool_args, ensure_ascii=False)}"
                    ),
                    {"tool": tool_name, "args": tool_args},
                )
            else:
                observation = await self._call_tool(ctx, tool_name, tool_args)

            messages.append({"role": "assistant", "content": response.content})
            messages.append({"role": "tool", "content": observation})

        return f"Reached the step limit ({self.MAX_STEPS}) without a final answer."

    # ── History persistence (HITL support) ───────────────────────────────────

    async def _save_history(self, task: dict, ctx, messages: list) -> None:
        """Persist the conversation history to agent memory.

        Called before suspending for HITL so the loop can resume exactly
        where it stopped. No-op if ctx.memory is None.
        """
        if ctx.memory is None:
            return
        task_id = task.get("task_id", "unknown")
        key = f"{_HISTORY_MEMORY_KEY_PREFIX}{task_id}"
        try:
            await ctx.memory.record(
                key,
                importance=MEMORY_IMPORTANCE_HISTORY,
                task_id=task_id,
                metadata={"history_json": json.dumps(messages)},
            )
        except Exception:
            # Memory errors must never crash the agent.
            pass

    async def _load_history(self, task: dict, ctx, ) -> list:
        """Restore conversation history from memory on HITL resume.

        Returns an empty list if not resuming, memory is unavailable, or
        the history entry was not found.
        """
        if not task.get("is_resumed") or ctx.memory is None:
            return []
        task_id = task.get("task_id", "unknown")
        key = f"{_HISTORY_MEMORY_KEY_PREFIX}{task_id}"
        try:
            entries = await ctx.memory.recall(key)
            if entries:
                history_json = entries[0].get("metadata", {}).get("history_json")
                if history_json:
                    return json.loads(history_json)
        except Exception:
            pass
        return []

    # ── Private helpers ───────────────────────────────────────────────────────

    def _initial_messages(
        self,
        user_message: str,
        available_tools: list,
        extra_context: str,
    ) -> list:
        """Build the opening messages for a fresh ReAct loop."""
        system = self._build_system_prompt(available_tools)
        content = user_message
        if extra_context:
            content = f"{extra_context}\n\n{user_message}"
        return [
            {"role": "system", "content": system},
            {"role": "user", "content": content},
        ]

    def _build_system_prompt(self, available_tools: list) -> str:
        """Combine SYSTEM_PROMPT with tool schemas and the output format contract."""
        tools_block = _build_tools_block(available_tools)
        return (
            f"{self.SYSTEM_PROMPT}\n\n"
            f"{tools_block}\n\n"
            "## Output format\n\n"
            "Respond with ONE JSON object per turn — no surrounding text.\n\n"
            "Tool call:\n"
            '  {"thought": "<why>", "action": "tool_call", '
            '"tool": "<name>", "args": {<parameters>}}\n\n'
            "Final answer (when you have enough information):\n"
            '  {"thought": "<why this is complete>", "action": "final_answer", '
            '"text": "<your complete response>"}\n\n'
            "Rules:\n"
            "- Call exactly one tool per turn.\n"
            "- Always include 'thought' to explain your reasoning.\n"
            "- Use 'final_answer' as soon as you have enough information.\n"
            "- Do not make up tool names — only use the tools listed above."
        )

    async def _call_tool(self, ctx, tool_name: str, tool_args: dict) -> str:
        """Execute a tool and return the observation as a plain string.

        Never raises — execution errors are returned as observation strings
        so the LLM can reason about them and try an alternative approach.
        """
        if ctx.tools is None:
            return (
                f"Error: ctx.tools is not available. "
                f"Add '{tool_name}' to your manifest's tools_required list."
            )
        try:
            result = await ctx.tools.call(tool_name, tool_args)
            raw = str(result)
            if len(raw) > OBSERVATION_TRUNCATE_CHARS:
                raw = (
                    raw[:OBSERVATION_TRUNCATE_CHARS]
                    + f"\n... [truncated — {len(raw)} chars total]"
                )
            return raw
        except Exception as exc:
            return f"Tool '{tool_name}' raised an error: {exc}"


# ─────────────────────────────────────────────────────────────────────────────
# HITL utilities
# ─────────────────────────────────────────────────────────────────────────────

def resume_pending_tool(task: dict) -> dict:
    """Extract the pending tool call from a resumed task.

    Returns None if the task was not resumed or has no pending tool context.
    This is a module-level helper intended for use inside run():

        pending = resume_pending_tool(task)
        result  = await self.react(task, ctx, user_msg, pending_tool=pending)
    """
    if not task.get("is_resumed"):
        return None
    ir = task.get("input_response")
    if ir is None:
        return None
    context = getattr(ir, "context", None) or {}
    tool_name = context.get("tool")
    tool_args = context.get("args", {})
    if not tool_name:
        return None
    return {"tool": tool_name, "args": tool_args}
