"""ctx.workflows - run another workflow of the installation as a step."""

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class WorkflowsInterface(Protocol):
    """Run an installed workflow and await what it returns.

    A workflow is a possible step of another workflow. ``inputs`` must match
    the ``TypedDict`` the called workflow declares.

    ``workflow_id`` is ``<package>/<workflow>``, or the bare name of a
    workflow of the caller's package. The runtime binds this service in a
    workflow run: the called workflow runs nested in the caller, stopped with
    it and spending from its budget, three levels deep at most. Reading
    ``ctx.workflows`` on any other runtime context raises ``AttributeError``.
    """

    # REASON(ANN401): the return is whatever the called workflow returns, a
    # shape only its author knows.
    async def run(self, workflow_id: str, inputs: dict[str, Any]) -> Any:  # noqa: ANN401
        """Run the workflow ``workflow_id`` with ``inputs`` and return its result."""
        ...
