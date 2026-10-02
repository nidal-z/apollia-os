"""ctx.agents - run another agent of the installation with a message."""

from typing import Protocol, runtime_checkable


@runtime_checkable
class AgentsInterface(Protocol):
    """Hand a message to an installed agent and await its answer.

    An agent is data: a mission, defaults, the tools it is granted. Running
    one from a workflow step or from a tool gives it ``message`` as the user
    turn of a fresh conversation, under its own grants, and returns its final
    answer once its loop ends.

    The runtime does not bind this service yet: reading ``ctx.agents`` on a
    runtime context raises ``AttributeError`` until it does.
    """

    async def run(self, agent_id: str, message: str) -> str:
        """Run the agent ``agent_id`` on ``message`` and return its final answer."""
        ...
