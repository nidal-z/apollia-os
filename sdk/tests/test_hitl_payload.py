"""Typed pauses in the SDK: the payload rides the pause, the answer reaches the code."""

from typing import Any

import pytest
from apollia import NeedHumanInput
from apollia.hitl import ApprovalPayload, QuestionPayload
from apollia.testing import MockContext

QUESTION: QuestionPayload = {
    "genre": "choix",
    "question": "Which list?",
    "propositions": [{"id": "a", "libelle": "Leads"}, {"id": "b", "libelle": "Clients"}],
}

APPROVAL: ApprovalPayload = {
    "genre": "approbation",
    "geste": "crm/delete",
    "risque": "critical",
    "detail": ["list: b"],
}


def test_need_human_input_carries_its_payload_and_its_context() -> None:
    # GIVEN a pause raised with a typed question and a context
    # WHEN it is built
    exc = NeedHumanInput("Which list?", {"step": 1}, payload=QUESTION)

    # THEN the prompt, the context and the payload are read back as given
    assert exc.prompt == "Which list?"
    assert exc.context == {"step": 1}
    assert exc.payload == QUESTION
    assert exc.from_tool_call is False


def test_a_prompt_only_pause_carries_no_payload() -> None:
    # GIVEN a pause raised without a payload nor a context
    # WHEN it is built
    exc = NeedHumanInput("Send?")

    # THEN the payload is absent and the context is empty rather than None
    assert exc.payload is None
    assert exc.context == {}


async def clean(ctx: Any) -> dict[str, Any]:
    """Ask which list, then ask to approve its deletion, then report both."""
    response = ctx.input_response
    if response is None:
        raise NeedHumanInput("Which list?", payload=QUESTION)
    if response["payload"]["genre"] == "choix":
        raise NeedHumanInput(
            "Delete it?",
            {"list": response["answer"]},
            payload=APPROVAL,
        )
    return {"list": response["context"]["list"], "approved": response["approved"]}


@pytest.mark.asyncio
async def test_code_resumed_twice_reads_each_answer_through_ctx() -> None:
    # GIVEN code that asks a question, then an approval, on a mock context
    ctx = MockContext()
    assert ctx.is_resumed is False

    # WHEN it runs, pauses, is resumed with an answer, pauses again, and is
    # resumed with the approval
    with pytest.raises(NeedHumanInput) as first:
        await clean(ctx)
    ctx.resume_with(answer="b", payload=first.value.payload)
    with pytest.raises(NeedHumanInput) as second:
        await clean(ctx)
    ctx.resume_with(
        approved=True,
        payload=second.value.payload,
        context=second.value.context,
    )
    third = await clean(ctx)

    # THEN it returns what it received across both pauses
    assert ctx.is_resumed is True
    assert second.value.payload == APPROVAL
    assert third == {"list": "b", "approved": True}
