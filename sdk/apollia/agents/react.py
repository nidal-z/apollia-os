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
from typing import Any, Iterator

from apollia.tools.schemas import (
    NATIVE_TOOL_SCHEMAS,
    build_tools_block,
    build_tools_block_from_ctx,
)
from apollia.utils.parsing import (
    ActionParseError,
    ACTION_FINAL_ANSWER,
    ACTION_TOOL_CALL,
    extract_json,
    validate_action,
)


# ---------------------------------------------------------------------------
# JSON-aware streaming filter
# ---------------------------------------------------------------------------


class _JsonFieldStreamer:
    """Incrementally extract deltas from specific JSON string fields.

    The LLM produces structured JSON like
    ``{"thought": "...", "action": "tool_call", "tool": "...", ...}``. Raw
    tokens are ugly to show to the user — we only surface the content of
    the ``thought`` and ``text`` fields (the parts humans read).

    Usage::

        streamer = _JsonFieldStreamer({"thought", "text"})
        for chunk in stream:
            for delta in streamer.feed(chunk):
                await ctx.emit_token(delta)

    The state machine is minimal and handles ``\\"`` / ``\\\\`` escapes.
    Other escapes (``\\n``, ``\\t``) pass through unchanged. Malformed JSON
    stops the streamer gracefully — nothing further is emitted.
    """

    # State constants.
    _SEEK = 0  # outside any string
    _KEY = 1  # inside a key string
    _AFTER_KEY = 2  # key closed, looking for ':'
    _BEFORE_VALUE = 3  # after ':', looking for '"'
    _IN_WATCHED = 4  # inside a watched field's string value — emit chars
    _IN_OTHER = 5  # inside an unwatched string value — skip chars

    def __init__(self, watched_fields: set[str]) -> None:
        self._watched = watched_fields
        self._state = self._SEEK
        self._current_key: list[str] = []
        self._escape = False
        self._closed = False

    def feed(self, chunk: str) -> Iterator[str]:
        """Feed new chars and yield deltas belonging to watched fields."""
        if self._closed:
            return
        for ch in chunk:
            # Escape handling inside any string.
            if self._escape:
                if self._state == self._IN_WATCHED:
                    yield ch
                # For keys and non-watched values we simply consume.
                self._escape = False
                continue
            if ch == "\\" and self._state in (
                self._KEY,
                self._IN_WATCHED,
                self._IN_OTHER,
            ):
                self._escape = True
                continue

            if self._state == self._SEEK:
                if ch == '"':
                    self._current_key = []
                    self._state = self._KEY
                # Ignore everything else at top level.
            elif self._state == self._KEY:
                if ch == '"':
                    self._state = self._AFTER_KEY
                else:
                    self._current_key.append(ch)
            elif self._state == self._AFTER_KEY:
                if ch == ":":
                    self._state = self._BEFORE_VALUE
                elif ch.isspace():
                    pass
                else:
                    # Unexpected — abort.
                    self._closed = True
                    return
            elif self._state == self._BEFORE_VALUE:
                if ch == '"':
                    key = "".join(self._current_key)
                    if key in self._watched:
                        self._state = self._IN_WATCHED
                    else:
                        self._state = self._IN_OTHER
                elif ch.isspace():
                    pass
                elif ch in ",}":
                    # Non-string value (missing / empty). Reset.
                    self._state = self._SEEK
                else:
                    # Non-string literal (number, bool, object…). We don't
                    # stream these — wait for next quoted key.
                    self._state = self._IN_OTHER
            elif self._state == self._IN_WATCHED:
                if ch == '"':
                    self._state = self._SEEK
                else:
                    yield ch
            elif self._state == self._IN_OTHER:
                if ch == '"':
                    self._state = self._SEEK


def _looks_like_broken_tool_call(text: str) -> bool:
    """Heuristic: does *text* look like a malformed tool_call JSON?

    Used to decide whether to feed the LLM a "your JSON was broken" hint
    on the next step instead of leaking the raw content to the user.
    """
    if not text:
        return False
    lowered = text.strip()
    if not lowered.startswith("{"):
        return False
    # Look for hallmark ReAct fields — enough evidence that the LLM was
    # trying to emit an action dict.
    markers = ('"action"', '"tool"', '"args"', '"thought"')
    return sum(1 for m in markers if m in lowered) >= 2


def _emit_safe(ctx: Any, method: str, *args: Any) -> None:
    """Best-effort call to a `ctx.emit_*` method (ADR-088, Lot 2).

    The runtime `RuntimeContext` exposes `emit_thought`, `emit_retry` and
    `emit_action_parse_error` as PyO3 methods. Older runtimes — or test
    contexts using `MockContext` — don't, so missing methods or any
    runtime error must NOT break the ReAct loop. This is pure telemetry.
    """
    method_fn = getattr(ctx, method, None)
    if method_fn is None:
        return
    try:
        method_fn(*args)
    except Exception:  # noqa: BLE001 — telemetry must never raise
        pass

# Maximum iterations enforced at the Python level.
# The Rust StepBudget is the authoritative hard limit.
DEFAULT_MAX_STEPS: int = 30

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
        history: list[dict[str, str]] | None = None,
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
            history:       Optional conversation history from previous user
                           turns (chat mode). Each entry is
                           ``{"role": "user"|"assistant", "content": "..."}``.
                           Inserted between the system prompt and the new
                           user_message. Ignored on HITL resume (the
                           memory-persisted intra-loop history takes over).
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
            messages = await self._initial_messages(
                ctx, user_message, available_tools, extra_context, history=history
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
        can_stream = self._ctx_supports_streaming(ctx)
        for step in range(self.MAX_STEPS):
            step_num = step + 1  # 1-based for human-friendly trace
            if can_stream:
                response_content = await self._stream_step(ctx, messages)
            else:
                response = await ctx.llm.complete(messages)
                response_content = response.content

            try:
                action = validate_action(extract_json(response_content))
            except ActionParseError:
                # ADR-088 Lot 2 — surface the parse failure on the trace bus
                # so the builder can see exactly what the LLM produced.
                _emit_safe(ctx, "emit_action_parse_error",
                           step_num, response_content, True)
                # The LLM produced output we can't parse as a ReAct action
                # even after heuristic JSON repair. This usually means a
                # tool_call with multi-line content and unescaped quotes
                # that the repair couldn't salvage. Don't dump the raw
                # JSON on the user — surface a readable explanation and
                # give the LLM a chance to correct itself on the next
                # step via the observation channel.
                if _looks_like_broken_tool_call(response_content):
                    _emit_safe(ctx, "emit_retry",
                               step_num, "action_parse_error", 1)
                    messages.append(
                        {"role": "assistant", "content": response_content}
                    )
                    messages.append({
                        "role": "tool",
                        "content": (
                            "Error: your previous response looked like a "
                            "tool_call but the JSON was malformed (likely "
                            "unescaped quotes or newlines in a long string "
                            "field). Retry with shorter content, or use "
                            "file_edit instead of file_write to add content "
                            "section by section."
                        ),
                    })
                    continue
                return response_content

            if action["action"] == ACTION_FINAL_ANSWER:
                return action["text"]

            tool_name: str = action["tool"]
            tool_args: dict[str, Any] = action.get("args", {})
            thought: str = action.get("thought", "")

            # ADR-088 Lot 2 — surface the LLM thought on the trace bus.
            # Empty thoughts are ignored to avoid trace noise.
            if thought:
                _emit_safe(ctx, "emit_thought", thought, step_num)

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

            messages.append({"role": "assistant", "content": response_content})
            messages.append({"role": "tool", "content": observation})

        return f"Reached the step limit ({self.MAX_STEPS}) without a final answer."

    # -- Streaming helpers -----------------------------------------------------

    @staticmethod
    def _ctx_supports_streaming(ctx: Any) -> bool:
        """Return True when the context has both streaming methods wired.

        Requires ``ctx.llm.stream_complete`` AND ``ctx.emit_token``. In
        task mode (CLI) neither is set, so the loop falls back to the
        non-streaming ``ctx.llm.complete`` path transparently.
        """
        llm = getattr(ctx, "llm", None)
        if llm is None:
            return False
        if not hasattr(llm, "stream_complete"):
            return False
        if not hasattr(ctx, "emit_token"):
            return False
        return True

    async def _stream_step(self, ctx: Any, messages: list[dict[str, Any]]) -> str:
        """Consume one LLM step in streaming mode.

        Feeds each chunk through a JSON-aware filter and emits only the
        deltas that belong to user-visible fields (``thought`` and
        ``final_answer.text``). The full accumulated JSON is returned
        unchanged so the caller can parse it with the existing
        ``validate_action(extract_json(...))`` pipeline.

        Falls back to ``complete()`` if the streaming API raises (e.g.
        backend not streaming-capable) so the loop always has a full
        response to parse.
        """
        try:
            stream = await ctx.llm.stream_complete(messages)
        except Exception:  # pragma: no cover — degradation path
            response = await ctx.llm.complete(messages)
            return response.content

        buffer: list[str] = []
        filter_ = _JsonFieldStreamer({"thought", "text"})
        try:
            async for chunk in stream:
                buffer.append(chunk)
                for delta in filter_.feed(chunk):
                    try:
                        await ctx.emit_token(delta)
                    except Exception:
                        # Best-effort: a broken event bus should never
                        # break the agent loop.
                        pass
        except Exception:  # pragma: no cover — degradation path
            # If the stream blows up mid-way but we accumulated some
            # text, hand it back to the parser; otherwise fall back to
            # complete().
            if not buffer:
                response = await ctx.llm.complete(messages)
                return response.content

        return "".join(buffer)

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
            await ctx.memory.remember(key, json.dumps(messages))
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
            history_json = await ctx.memory.recall(key)
            if not history_json:
                return []
            return json.loads(history_json)
        except Exception:
            pass
        return []

    # -- Private helpers -------------------------------------------------------

    async def _initial_messages(
        self,
        ctx: Any,
        user_message: str,
        available_tools: list[str],
        extra_context: str,
        *,
        history: list[dict[str, str]] | None = None,
    ) -> list[dict[str, str]]:
        """Build the opening messages for a fresh ReAct loop.

        When ``history`` is provided, its entries are inserted between the
        system prompt and the new user_message so the LLM sees prior
        conversational turns (chat mode multi-turn).
        """
        system = await self._build_system_prompt(ctx, available_tools)
        content = user_message
        if extra_context:
            content = f"{extra_context}\n\n{user_message}"
        messages: list[dict[str, str]] = [{"role": "system", "content": system}]
        if history:
            messages.extend(history)
        messages.append({"role": "user", "content": content})
        return messages

    async def _build_system_prompt(
        self, ctx: Any, available_tools: list[str]
    ) -> str:
        """Combine SYSTEM_PROMPT with tool schemas and the output format.

        Tool schemas are pulled live from the runtime tool registry via
        ``ctx.tools.describe()``. The static SDK mirror is only used as a
        fallback when ``ctx.tools`` is unavailable (tests, dry-run).
        """
        tools_block = await build_tools_block_from_ctx(ctx, available_tools)
        return (
            f"{self.SYSTEM_PROMPT}\n\n"
            f"{tools_block}\n\n"
            "## Output format\n\n"
            "Respond with ONE JSON object per turn — no surrounding text, "
            "no markdown fences.\n\n"
            "Tool call:\n"
            '  {"thought": "<why>", "action": "tool_call", '
            '"tool": "<tool_name>", "args": {<parameters>}}\n\n'
            "Final answer (when you have enough information):\n"
            '  {"thought": "<why this is complete>", "action": "final_answer", '
            '"text": "<your complete response>"}\n\n'
            "Concrete example calling `ask_user`:\n"
            '  {"thought": "I need the target stack before writing the spec.", '
            '"action": "tool_call", "tool": "ask_user", '
            '"args": {"questions": [{"id": "stack", "question": "Which '
            'framework?", "type": "open"}]}}\n\n'
            "Rules:\n"
            "- The `action` field is ALWAYS exactly `tool_call` or "
            "`final_answer` — never a tool name. The tool name goes in "
            "the `tool` field.\n"
            "- Call exactly one tool per turn.\n"
            "- Always include `thought` to explain your reasoning.\n"
            "- Use `final_answer` as soon as you have enough information.\n"
            "- Do not invent tool names — only use the tools listed above."
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
