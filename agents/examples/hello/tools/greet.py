"""The tool of the package hello.

A tool is an async function under ``@tool``. Its first parameter is the
context the runtime hands it; the others are its inputs, described with
``Annotated`` so the model knows what to pass.
"""

from typing import Annotated

from apollia import tool
from apollia.types import Ctx


@tool(
    "greet",
    "Build a greeting for a person, from their name.",
    action="read",
    examples=[{"name": "Ada"}],
)
async def greet(
    ctx: Ctx,
    name: Annotated[str, "The name of the person to greet."],
) -> dict[str, str]:
    """Return the greeting for ``name``."""
    # `ctx.logger` is a stdlib `logging.Logger` routed into the runtime tracer:
    # it takes printf-style arguments, not keyword fields.
    ctx.logger.info("greet called for a name of %d characters", len(name))
    return {"greeting": f"Hello, {name.strip()}!"}
