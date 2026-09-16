"""Typed pauses: what an agent asks a human, and what it gets back.

An agent pauses by raising :class:`~apollia.errors.NeedHumanInput`. With a
``payload`` the pause is typed: a question with named propositions, or an
approval of a named gesture with its risk. The runtime checks the payload when
the agent pauses (an invalid one fails the task with ``INVALID_INPUT_PAYLOAD``)
and checks the operator's answer against it when the task is resumed.

When the task resumes, the agent runs again from the top and reads
:attr:`Ctx.input_response`::

    from apollia import NeedHumanInput, skill
    from apollia.hitl import QuestionPayload

    @skill("pick.list")
    async def pick(self, ctx):
        response = ctx.input_response
        if response is None:
            question: QuestionPayload = {
                "genre": "choix",
                "question": "Which list should I clean?",
                "propositions": [
                    {"id": "a", "libelle": "Leads"},
                    {"id": "b", "libelle": "Clients"},
                ],
            }
            raise NeedHumanInput("Which list?", payload=question)
        return {"list": response["answer"]}

The keys are the product's vocabulary (``genre``, ``libelle``, ``portee``...)
and are kept as the product defined them.

This module declares ``TypedDict`` and must not use postponed annotations: the
runtime reads ``__required_keys__``.
"""

from typing import Any, Literal, TypedDict


class Proposition(TypedDict):
    """One answer the human can pick."""

    id: str
    libelle: str


class _QuestionRequired(TypedDict):
    genre: Literal["choix", "source", "seuil", "definition", "confirmation"]
    question: str


class QuestionPayload(_QuestionRequired, total=False):
    """A typed question.

    * ``choix``: pick one of ``propositions``; needs at least one, or ``autre``.
    * ``source``, ``definition``: a proposition id, or free text when ``autre``.
    * ``seuil``: a number, or a proposition id.
    * ``confirmation``: a boolean, or a proposition id.

    ``autre`` accepts free text outside the propositions. ``portee`` says what
    the answer applies to. ``memoire`` asks for the answer to be remembered.
    """

    propositions: list[Proposition]
    autre: bool
    portee: str
    memoire: bool


class _ApprovalRequired(TypedDict):
    genre: Literal["approbation"]
    geste: str
    risque: Literal["low", "medium", "critical"]


class ApprovalPayload(_ApprovalRequired, total=False):
    """An approval of one named gesture.

    ``detail`` lists what exactly will happen. ``delai`` is how long the request
    stays valid, in seconds. The decision is ``approved`` alone: an approval
    takes no ``answer``.
    """

    detail: list[str]
    delai: int


#: The payload a pause may carry.
HitlPayload = QuestionPayload | ApprovalPayload


class InputResponse(TypedDict):
    """What a resumed agent reads from :attr:`Ctx.input_response`.

    ``approved`` and ``reason`` are the operator's decision. ``answer`` is the
    proposition id, free text or value given to a question, ``None`` for an
    approval. ``payload`` is the pause being answered, so an agent that pauses
    more than once knows which pause it is resuming from. ``context`` is the
    ``context`` it passed to :class:`~apollia.errors.NeedHumanInput`, given back
    verbatim: the way to carry what an earlier pause learned into a later one.
    """

    approved: bool
    reason: str | None
    answer: Any
    payload: HitlPayload | None
    context: dict[str, Any]


__all__ = [
    "ApprovalPayload",
    "HitlPayload",
    "InputResponse",
    "Proposition",
    "QuestionPayload",
]
