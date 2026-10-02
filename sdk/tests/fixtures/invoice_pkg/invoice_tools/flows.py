"""Workflows of the example package."""

from typing import Annotated, Any, NotRequired, TypedDict

from apollia import WorkflowCtx, workflow


class MonthClose(TypedDict):
    """Inputs of the close-month workflow."""

    folder: Annotated[str, "folder holding the month's invoices"]
    notify: NotRequired[bool]


@workflow(
    "close-month",
    "Extract every invoice of a folder and file the totals.",
    inputs=MonthClose,
    steps=["extract", "file"],
    action="write_or_delete",
)
async def close_month(ctx: WorkflowCtx, inputs: MonthClose) -> dict[str, Any]:
    async with ctx.step("extract"):
        totals = await ctx.tools.call("extract_totals", {"path": inputs["folder"]})
    async with ctx.step("file"):
        await ctx.tools.call("file_totals", {"totals": [totals["total"]]})
    return totals
