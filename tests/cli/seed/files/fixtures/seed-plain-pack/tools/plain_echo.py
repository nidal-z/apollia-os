"""The tool of the seed fixture seed-plain-pack."""

from typing import Annotated, Any

from apollia import tool
from apollia.types import Ctx


@tool(
    "plain_echo",
    "Echo the text it is given.",
    action="read",
)
async def plain_echo(
    ctx: Ctx,
    text: Annotated[str, "The text to echo."],
) -> dict[str, Any]:
    ctx.logger.info("plain_echo called")
    return {"echo": text}
