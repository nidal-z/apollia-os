"""Workflow of the package used by test_hitl_e2e.py to pause on questions.

The module of the package ``hitl-e2e``. Its one workflow, ``purge``, asks the
user twice, each time from inside a step:

* ``choose`` asks which list to purge and proposes two; the answer is one of
  them or free text, and is not caught when declined, so a refusal fails the
  step and the run.
* ``confirm`` asks to confirm the purge of the list just chosen, a question
  built from the first answer; a refusal is caught as ``QuestionDeclined`` and
  returned with its reason.
* ``purge`` returns everything the two answers gave.

The file keeps the name the end-to-end suite has always loaded; it declares no
agent.
"""

from typing import Any, TypedDict

from apollia import WorkflowCtx, workflow
from apollia.errors import QuestionDeclined


class Purge(TypedDict):
    crm: str


@workflow(
    "purge",
    "Purge a list of a CRM, after asking which one and a confirmation.",
    inputs=Purge,
    steps=["choose", "confirm", "purge"],
    action="write_or_delete",
)
async def purge(ctx: WorkflowCtx, inputs: Purge) -> dict[str, Any]:
    """Ask which list, ask to confirm, then return both answers."""
    async with ctx.step("choose"):
        chosen = await ctx.user.ask(
            "Which list should be purged?",
            options=["leads", "clients"],
            hint="a list of the CRM",
        )
    async with ctx.step("confirm"):
        try:
            confirmation = await ctx.user.ask(
                f"Purge the list {chosen} of {inputs['crm']}?", options=["yes", "no"]
            )
        except QuestionDeclined as declined:
            return {
                "list": chosen,
                "purged": False,
                "declined_question": declined.question,
                "decline_reason": declined.reason,
            }
    async with ctx.step("purge"):
        return {"list": chosen, "purged": confirmation == "yes", "confirmation": confirmation}
