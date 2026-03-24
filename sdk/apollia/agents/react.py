"""BaseReActAgent — Abstract base class for ReAct agents on Apollia OS.

Implements the Reason-Act-Observe loop using ``ctx.llm`` and ``ctx.tools``.
Subclasses define a ``SYSTEM_PROMPT`` and implement ``manifest()`` + ``run()``.

ReAct loop
----------

.. code-block:: text

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
of the system prompt).  Unstructured text is treated as a final answer.

HITL support
------------

Tools listed in ``manifest()["tools_requiring_approval"]`` trigger a
suspension.  On resume (``task["is_resumed"] = True``), the agent reads
``task["input_response"]`` and either executes the deferred tool or aborts.

If ``ctx.memory`` is available, the conversation history is persisted across
suspension so the loop continues exactly where it stopped.

Graceful degradation
--------------------

- ``ctx.llm is None`` → ``AIPResult.failed("NO_LLM", ...)``
- ``ctx.tools is None`` → loop still works; all tool calls return an error observation
- ``ctx.memory is None`` → HITL resumes from a fresh loop (history is lost)
"""

from __future__ import annotations

import json
from abc import ABC, abstractmethod
from typing import Any

from apollia.tools.schemas import NATIVE_TOOL_SCHEMAS, build_tools_block
from apollia.utils.parsing import (
    ActionParseError,
    ACTION_FINAL_ANSWER,
    ACTION_TOOL_CALL,
    extract_json,
    validate_action,
)

# Maximum iterations enforced at the Python level.
# The Rust StepBudget is the authoritative hard limit.
DEFAULT_MAX_STEPS: int = 10

# Observations longer than this are truncated before being fed back to the LLM.
OBSERVATION_TRUNCATE_CHARS: int = 2000

# Memory importance score for conversation history snapshots.
MEMORY_IMPORTANCE_HISTORY: float = 0.7

# Memory key prefix used to store conversation history for HITL resume.
_HISTORY_MEMORY_KEY_PREFIX: str = "react_history:"


class AIPResult:
    """Factory for the result dicts returned by agent ``run()`` methods.

    The Rust bridge deserialises the dict into ``apollia_core::AIPResult``.
    All fields are optional on the Rust side (serde default), so only the
    relevant fields need to be set per status.

    Usage::

        return AIPResult.completed("Final answer text")
        return AIPResult.failed("ERROR_CODE", "Human-readable message")
        return AIPResult.input_required("Approve X?", {"tool": "bash", "args": {}})
    """

    @staticmethod
    def completed(text: str) -> dict[str, Any]:
        """Return a successful result with a plain-text output."""
        return {
            "status": "completed",
            "output": [{"type": "text", "text": str(text)}],
        }

    @staticmethod
    def failed(
        code: str,
        message: str,
        details: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Return a failed result with an error code and message."""
        error: dict[str, Any] = {"code": str(code), "message": str(message)}
        if details is not None:
            error["details"] = details
        return {
            "status": "failed",
            "error": error,
        }

    @staticmethod
    def input_required(
        prompt: str,
        context: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Suspend the task and request human approval (HITL).

        The runtime delivers the approval decision back via
        ``task["is_resumed"]`` and ``task["input_response"]`` on the next call.
        """
        return {
            "status": "input_required",
            "input_required_data": {
                "prompt": str(prompt),
                "context": context or {},
            },
        }


class BaseReActAgent(ABC):
    """Abstract base class for ReAct agents running on Apollia OS (Mode Direct).

    Subclassing
    -----------

    Override ``SYSTEM_PROMPT`` and implement ``manifest()`` and ``run()``::

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
                pending = resume_pending_tool(task)
                result = await self.react(task, ctx, user_msg, pending_tool=pending)
                if isinstance(result, dict):
                    return result
                return AIPResult.completed(result)

    HITL pattern
    ------------

    When a tool requires approval, ``react()`` returns
    ``AIPResult.input_required()``.  The runtime suspends the task.
    On resume:

    - ``task["is_resumed"]`` is ``True``
    - ``task["input_response"].approved`` tells you the decision
    - ``task["input_response"].context`` holds ``{"tool": ..., "args": ...}``

    Call ``resume_pending_tool(task)`` to extract the pending tool, then
    pass it to ``react()`` via ``pending_tool=``.
    """

    SYSTEM_PROMPT: str = "You are a helpful assistant."
    MAX_STEPS: int = DEFAULT_MAX_STEPS
    TEMPERATURE: float = 0.3

    @abstractmethod
    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest dict."""

    @abstractmethod
    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Main entry point called once per task by the Apollia runtime."""

    def get_tool_schemas(self) -> list[dict[str, Any]]:
        """Return tool schemas available to this agent."""
        return list(NATIVE_TOOL_SCHEMAS.values())

    async def react(
        self,
        task: dict[str, Any],
        ctx: Any,
        user_message: str,
        *,
        extra_context: str = "",
        pending_tool: dict[str, Any] | None = None,
    ) -> str | dict[str, Any]:
        """Run the ReAct loop and return the final answer or an AIPResult dict.

        Returns:
            ``str`` — final answer text on success.
            ``dict`` — AIPResult dict (input_required or failed) to be returned
            directly from ``run()``.

        Args:
            task:          AIP task dict from the runtime.
            ctx:           RuntimeContext with ctx.llm, ctx.tools, ctx.memory.
            user_message:  The user's request.
            extra_context: Additional context prepended to user_message.
            pending_tool:  ``{"tool": str, "args": dict}`` from a HITL resume.
        """
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "No LLM backend configured — cannot run the ReAct loop.",
            )

        available_tools: list[str] = ctx.tools.list_tools() if ctx.tools else []
        tools_requiring_approval: set[str] = set(
            self.manifest().get("tools_requiring_approval", [])
        )

        # Try to restore conversation history if this is a HITL resume.
        messages: list[dict[str, Any]] = await self._load_history(task, ctx)
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

        # Main ReAct loop.
        for step in range(self.MAX_STEPS):
            response = await ctx.llm.complete(messages)

            try:
                action = validate_action(extract_json(response.content))
            except ActionParseError:
                return response.content

            if action["action"] == ACTION_FINAL_ANSWER:
                return action["text"]

            tool_name: str = action["tool"]
            tool_args: dict[str, Any] = action.get("args", {})
            thought: str = action.get("thought", "")

            if tool_name not in available_tools:
                observation = (
                    f"Error: tool '{tool_name}' is not in your allowed list. "
                    f"Allowed tools: {available_tools}"
                )
            elif tool_name in tools_requiring_approval:
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

    # -- History persistence (HITL support) ------------------------------------

    async def _save_history(
        self,
        task: dict[str, Any],
        ctx: Any,
        messages: list[dict[str, Any]],
    ) -> None:
        """Persist conversation history to agent memory before HITL suspension.

        No-op if ``ctx.memory is None``.
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
            pass

    async def _load_history(
        self,
        task: dict[str, Any],
        ctx: Any,
    ) -> list[dict[str, Any]]:
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

    # -- Private helpers -------------------------------------------------------

    def _initial_messages(
        self,
        user_message: str,
        available_tools: list[str],
        extra_context: str,
    ) -> list[dict[str, str]]:
        """Build the opening messages for a fresh ReAct loop."""
        system = self._build_system_prompt(available_tools)
        content = user_message
        if extra_context:
            content = f"{extra_context}\n\n{user_message}"
        return [
            {"role": "system", "content": system},
            {"role": "user", "content": content},
        ]

    def _build_system_prompt(self, available_tools: list[str]) -> str:
        """Combine SYSTEM_PROMPT with tool schemas and the output format."""
        tools_block = build_tools_block(available_tools)
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

    async def _call_tool(
        self,
        ctx: Any,
        tool_name: str,
        tool_args: dict[str, Any],
    ) -> str:
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
