"""ctx.agents - run another agent of the installation with a message."""

from typing import Protocol, runtime_checkable


@runtime_checkable
class AgentsInterface(Protocol):
    """Hand a message to an installed agent and await its answer.

    An agent is data: a mission, defaults, the tools it is granted. Running
    one from a workflow step or from a tool gives it ``message`` as the user
    turn of a fresh conversation, under its own grants, and returns its final
    answer once its loop ends.

    The runtime binds this service in a workflow run: the agent runs nested
    in the run, stopped with it and spending from its budget. Reading
    ``ctx.agents`` on any other runtime context raises ``AttributeError``.
    """

    async def run(self, agent_id: str, message: str) -> str:
        """Run the agent ``agent_id`` on ``message`` and return its final answer."""
        ...
