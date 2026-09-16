"""Typed pauses in the SDK: the payload reaches the runtime, the answer reaches the agent."""

import pytest
from apollia import NeedHumanInput, agent, skill
from apollia._internal.aip_result import from_exception, from_handler_return
from apollia.hitl import ApprovalPayload, QuestionPayload
from apollia.testing import mock
from apollia.types import AIPResult

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


def test_need_human_input_carries_its_payload_to_the_runtime_shape() -> None:
    # GIVEN a pause raised with a typed question
    exc = NeedHumanInput("Which list?", {"step": 1}, payload=QUESTION)

    # WHEN the dispatch boundary maps it
    result = from_exception(exc)

    # THEN the runtime receives the payload beside the prompt and the context
    assert result["status"] == "input_required"
    data = result["input_required_data"]
    assert data["prompt"] == "Which list?"
    assert data["context"] == {"step": 1}
    assert data["payload"] == QUESTION


def test_a_prompt_only_pause_keeps_its_historical_shape() -> None:
    # GIVEN a pause raised without a payload
    result = from_exception(NeedHumanInput("Send?"))

    # WHEN it reaches the runtime shape
    # THEN no payload key appears, so the runtime reads it as it always did
    assert "payload" not in result["input_required_data"]


def test_aip_result_input_required_returned_from_a_skill_pauses_the_task() -> None:
    # GIVEN the public factory, returned rather than raised
    value = AIPResult.input_required("Delete?", {"list": "b"}, payload=APPROVAL)

    # WHEN a handler return is coerced
    result = from_handler_return(value)

    # THEN the task pauses. Read as a dataclass it used to complete, carrying
    # the pause as data, so the factory never paused anything.
    assert result["status"] == "input_required"
    assert result["input_required_data"]["payload"] == APPROVAL
    assert result["input_required_data"]["context"] == {"list": "b"}


def test_legacy_completed_and_failed_results_keep_their_meaning() -> None:
    # GIVEN the other two public factories, returned from a handler
    # WHEN each is coerced
    done = from_handler_return(AIPResult.completed("ok"))
    broken = from_handler_return(AIPResult.failed("NOPE", "no"))

    # THEN each lands on its own status rather than on a completed data blob
    assert done["status"] == "completed"
    assert broken["status"] == "failed"
    assert broken["error"]["code"] == "NOPE"


@agent(name="two-pause-agent", version="1.0.0", description="asks, then asks for approval")
class TwoPauseAgent:
    @skill("clean.list")
    async def clean(self, ctx) -> dict:  # type: ignore[no-untyped-def]
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
async def test_an_agent_resumed_twice_reads_each_answer_through_ctx() -> None:
    # GIVEN an agent that asks a question, then an approval, on a mock context
    agent_instance, ctx = mock(TwoPauseAgent)
    assert ctx.is_resumed is False

    # WHEN it runs, pauses, is resumed with an answer, pauses again, and is
    # resumed with the approval
    first = await agent_instance.invoke_skill("clean.list")
    assert first["status"] == "input_required"
    ctx.resume_with(answer="b", payload=first["input_required_data"]["payload"])
    second = await agent_instance.invoke_skill("clean.list")
    assert second["status"] == "input_required"
    ctx.resume_with(
        approved=True,
        payload=second["input_required_data"]["payload"],
        context=second["input_required_data"]["context"],
    )
    third = await agent_instance.invoke_skill("clean.list")

    # THEN it returns what it received across both pauses
    assert ctx.is_resumed is True
    assert third["status"] == "completed"
    assert third["output"][0]["data"] == {"list": "b", "approved": True}
