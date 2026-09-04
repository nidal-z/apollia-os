"""ctx.a2a - unified Agent-to-Agent invocation."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class SkillCard(Protocol):
    """Discovered skill metadata returned by :meth:`A2AInterface.discover`."""

    skill_id: str
    name: str
    description: str
    agent_name: str
    input_schema: dict[str, Any]
    output_schema: dict[str, Any]


@runtime_checkable
class A2AInterface(Protocol):
    """Inter-agent invocation API.

    ``ctx.a2a.invoke`` is the single typed entry point for a synchronous
    agent-to-agent call: it runs a skill on another agent and awaits a typed
    result. Asynchronous, non-blocking messaging lives on a separate service,
    ``ctx.mail``.
    """

    async def invoke(
        self,
        skill_id: str,
        input: dict[str, Any] | None,
        *,
        timeout_secs: int | None = None,
    ) -> dict[str, Any]:
        """Invoke an A2A skill and return the full invocation envelope.

        On success the return value is the A2A envelope, not the skill's
        payload directly::

            {
                "result": {
                    "task_id": "...",
                    "status": "completed",
                    "output": [{"type": "data", "data": {...}}],
                    "error": None,
                    "artifacts": [],
                    "input_required_data": None,
                },
                "agent_name": "...",
                "skill_id": "...",
                "duration_ms": 123,
            }

        The skill's returned dict lives at ``result.output[0].data``. Use
        :func:`apollia.utils.formatting.a2a_result_data` to unwrap it, or
        :func:`apollia.utils.formatting.aip_result_text` on ``result`` for the
        text parts. ``timeout_secs=None`` uses the backend default.

        On error the return value is a failed ``AIPResult`` dict
        (``{"status": "failed", "error": {...}, ...}``), not the envelope.
        """
        ...

    async def discover(self, skill_id: str) -> dict[str, Any] | None:
        """Return the skill card for ``skill_id``, or None if unknown."""
        ...

    async def list_skills(self) -> list[dict[str, Any]]:
        """Return a skill card for every skill reachable from this context."""
        ...

    async def skill_as_tool(self, skill_id: str) -> dict[str, Any]:
        """Return an LLM tool descriptor for an A2A skill.

        The descriptor the bridge emits is
        ``{"name", "description", "input_schema"}``, plus ``"examples"`` when
        the skill ships sample payloads. ``name`` is the encoded form
        ``a2a__<skill_id>`` with every dot replaced by a double underscore,
        which is what the bridge decodes back on dispatch.

        The schema key is ``input_schema``, which is the Anthropic spelling.
        ``ctx.llm.run_tools`` and :func:`apollia.react` read ``parameters``
        first and fall back to ``input_schema``, so this descriptor can be
        handed to either as it stands and the tool reaches the model with its
        arguments. The bridge read only ``parameters`` until it was fixed: a
        descriptor built by hand for an older runtime should spell the key
        ``parameters``, which still takes precedence.

        The method is ``async``: the bridge resolves the skill against
        the in-process A2A registry. Always call with ``await``.
        """
        ...
