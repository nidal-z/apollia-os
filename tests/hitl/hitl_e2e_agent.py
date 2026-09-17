"""Agent used by test_hitl_e2e.py to pause on the task path and read the answers.

Two skills, one per property the end-to-end test proves:

* ``hitl.ask`` asks a typed question, then asks for an approval, and returns
  everything it received across both resumes.
* ``hitl.mcp`` calls a tool on an MCP server declared ``requires_approval``. The
  call pauses the task; resumed approved, it runs; resumed declined, it raises
  ``ToolApprovalDenied``, which the skill catches and returns.
* ``hitl.card_then_write`` raises its own card carrying its state, and once
  approved makes the gated call without catching the engine's pause. Resumed
  from that second pause, it must still read its state.
"""

from apollia import NeedHumanInput, agent, skill
from apollia.errors import ToolApprovalDenied
from apollia.hitl import ApprovalPayload, QuestionPayload
from apollia.types import Ctx

QUESTION: QuestionPayload = {
    "genre": "choix",
    "question": "Which list should be purged?",
    "propositions": [
        {"id": "leads", "libelle": "Leads"},
        {"id": "clients", "libelle": "Clients"},
    ],
    "autre": False,
    "portee": "this run",
    "memoire": False,
}


def purge_approval(target: str) -> ApprovalPayload:
    """The approval asked once the list is known."""
    return {
        "genre": "approbation",
        "geste": "crm/purge",
        "risque": "critical",
        "detail": [f"list: {target}"],
        "delai": 600,
    }


@agent(
    name="hitl-e2e-agent",
    version="1.0.0",
    description="Pauses on the task path and returns what it was answered.",
    tools_required=("mcp:calc/add",),
)
class HitlE2EAgent:
    """End-to-end subject for typed pauses."""

    @skill("hitl.ask", description="Ask a question, then an approval.")
    async def ask(self, ctx: Ctx = None) -> dict:  # type: ignore[assignment]
        """Pause twice, then return both answers."""
        response = ctx.input_response
        if response is None:
            raise NeedHumanInput("Which list should be purged?", payload=QUESTION)
        if response["payload"]["genre"] == "choix":
            chosen = response["answer"]
            raise NeedHumanInput(
                f"Purge the list {chosen}?",
                {"list": chosen},
                payload=purge_approval(chosen),
            )
        return {
            "is_resumed": ctx.is_resumed,
            "list": response["context"]["list"],
            "purge_approved": response["approved"],
            "purge_reason": response["reason"],
            "answered_gesture": response["payload"]["geste"],
        }

    @skill("hitl.mcp", description="Call a tool whose server requires approval.")
    async def mcp(self, ctx: Ctx = None) -> dict:  # type: ignore[assignment]
        """Call the gated tool; the call itself pauses the task."""
        try:
            out = await ctx.tools.call("mcp:calc/add", {"a": 2, "b": 3})
        except ToolApprovalDenied as denied:
            return {"mcp": "denied", "tool": denied.tool, "reason": denied.reason}
        return {"mcp": out["content"]}

    @skill("hitl.card_then_write", description="Own card, then a gated write.")
    async def card_then_write(self, ctx: Ctx = None) -> dict:  # type: ignore[assignment]
        """The shape of a generated write flow: a card per record, then the write."""
        response = ctx.input_response
        if response is None:
            raise NeedHumanInput(
                "Write record r-1?",
                {"records": ["r-1"], "index": 0},
                payload={
                    "genre": "approbation",
                    "geste": "espace/write",
                    "risque": "medium",
                    "detail": ["r-1"],
                },
            )
        state = response["context"]
        record = state["records"][state["index"]]
        out = await ctx.tools.call("mcp:calc/add", {"a": 2, "b": 3})
        return {"record": record, "mcp": out["content"]}


agent = HitlE2EAgent()
