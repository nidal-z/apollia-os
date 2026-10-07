"""The workflow of the package echo.

It returns the message it receives, in one step and with no model call, so a
host program can drive a real runtime end to end on any machine.
"""

from typing import Annotated, TypedDict

from apollia import WorkflowCtx, workflow


class EchoInputs(TypedDict):
    """Inputs of the echo workflow."""

    message: Annotated[str, "The message to return."]


class EchoResult(TypedDict):
    """Result of the echo workflow."""

    echo: str


@workflow(
    "echo",
    "Return the message it receives.",
    inputs=EchoInputs,
    steps=["echo"],
    action="read",
)
async def echo(ctx: WorkflowCtx, inputs: EchoInputs) -> EchoResult:
    """Return ``inputs["message"]`` under the key ``echo``."""
    async with ctx.step("echo"):
        ctx.logger.info("echo received %d characters", len(inputs["message"]))
        return {"echo": inputs["message"]}
