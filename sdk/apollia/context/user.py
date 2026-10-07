"""ctx.user - ask the user a question in the middle of a workflow step."""

from typing import Protocol, runtime_checkable


@runtime_checkable
class UserInterface(Protocol):
    """Stop the run on a question and go on with the user's answer.

    The workflow counterpart of the ``ask_user`` tool an agent calls in a
    conversation: one question, the choices it proposes when it has some, and
    a free answer the user may always give instead of a choice.

    The runtime binds this service in a workflow run. ``ask`` waits, inside
    the step that calls it, until the user answers: the question is listed
    with the decisions waiting for the user, with the run, the workflow and
    the step it comes from, and the answer given there is what ``ask``
    returns. Stopping the run ends the wait. The wait spends the wall clock
    of the run's budget, and a run that waits when the runtime stops is
    failed at the next start, as every interrupted run is. Reading
    ``ctx.user`` on any other runtime context raises ``AttributeError``.
    """

    async def ask(
        self,
        question: str,
        *,
        options: list[str] | None = None,
        hint: str | None = None,
    ) -> str:
        """Ask ``question`` and return the user's answer.

        Args:
            question: What is asked, as the user reads it.
            options: The proposed choices, at most 20; the answer is one of
                them or free text, so check it before branching on it.
            hint: A placeholder for the free answer.

        Raises:
            QuestionDeclined: The user declined to answer. Catch it to go on
                without the answer; left uncaught, the step and the run fail.
            PayloadError: ``question``, ``options`` or ``hint`` is not text.
        """
        ...
