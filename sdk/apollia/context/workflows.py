"""ctx.workflows - run another workflow of the installation as a step."""

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class WorkflowsInterface(Protocol):
    """Run an installed workflow and await what it returns.

    A workflow is a possible step of another workflow. ``inputs`` must match
    the ``TypedDict`` the called workflow declares.

    The runtime does not bind this service yet: reading ``ctx.workflows`` on
    a runtime context raises ``AttributeError`` until it does.
    """

    # REASON(ANN401): the return is whatever the called workflow returns, a
    # shape only its author knows.
    async def run(self, workflow_id: str, inputs: dict[str, Any]) -> Any:  # noqa: ANN401
        """Run the workflow ``workflow_id`` with ``inputs`` and return its result."""
        ...
