"""The tool of the seed fixture seed-deps-pack."""

from typing import Annotated, Any

from apollia import tool
from apollia.types import Ctx


@tool(
    "deps_echo",
    "Echo the text it is given.",
    action="read",
)
async def deps_echo(
    ctx: Ctx,
    text: Annotated[str, "The text to echo."],
) -> dict[str, Any]:
    ctx.logger.info("deps_echo called")
    return {"echo": text}
