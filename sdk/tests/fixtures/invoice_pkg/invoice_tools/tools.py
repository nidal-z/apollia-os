"""Tools of the example package."""

from typing import Annotated, Any

from apollia import tool


@tool(
    "extract_totals",
    "Read the totals of one invoice.",
    action="read",
    examples=[{"path": "invoices/2026-09.pdf"}],
)
async def extract_totals(ctx: Any, path: Annotated[str, "path of the invoice"]) -> dict[str, float]:
    return {"total": 0.0}


@tool("file_totals", 'File totals in the "ledger" sheet.', action="write_or_delete")
async def file_totals(ctx: Any, totals: list[float], sheet: str = "ledger") -> int:
    return len(totals)


def helper() -> None:
    """Not a tool: describe ignores it."""
