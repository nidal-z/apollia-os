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

    Consolidates the legacy ``ctx.send`` / ``ctx.receive`` / ``ctx.delegate``
    / ``ctx.a2a_invoke`` surfaces into a single, typed entry point.
    """

    async def invoke(
        self,
        skill_id: str,
        input: dict[str, Any] | None = None,
        *,
        timeout_secs: int = 120,
        **kwargs: Any,
    ) -> dict[str, Any]: ...

    async def discover(self, skill_id: str) -> dict[str, Any] | None: ...

    async def list_skills(self) -> list[dict[str, Any]]: ...

    async def skill_as_tool(self, skill_id: str) -> dict[str, Any]:
        """Return an LLM tool descriptor for an A2A skill.

        The descriptor follows the Anthropic / OpenAI ``tool-use``
        convention (``{"name", "description", "input_schema"}``) and is
        intended to be passed to :func:`apollia.react` or directly to
        ``ctx.llm.run_tools``.

        The method is ``async``: the bridge resolves the skill against
        the in-process A2A registry. Always call with ``await``::

            tools = [
                await ctx.a2a.skill_as_tool("pdf.read_text"),
                await ctx.a2a.skill_as_tool("web.search"),
            ]
        """
        ...
