"""Tests for the input types of ``@workflow``: date, long text and file."""

import datetime
from typing import Annotated, Any, Literal, NotRequired, TypedDict

import pytest
from apollia import FileInput, LongText, WorkflowCtx, workflow
from apollia.errors import AgentConfigError, PayloadError
from apollia.testing import MockContext
from apollia.types import DATE_FORMAT, FILE_PATH_FORMAT, LONG_TEXT_FORMAT
from apollia.workflow_decorator import INPUT_ORDER_KEY, WORKFLOW_ATTR, WorkflowSpec


class Reminder(TypedDict):
    invoice: Annotated[FileInput, "the unpaid invoice"]
    due: datetime.date
    message: NotRequired[LongText]
    days: int
    channel: Literal["mail", "letter"]
    copies: NotRequired[list[str]]


def _remind() -> Any:
    @workflow(
        "remind",
        "Send a reminder for an unpaid invoice.",
        inputs=Reminder,
        steps=["write"],
        action="read",
    )
    async def remind(ctx: WorkflowCtx, inputs: Reminder) -> dict[str, Any]:
        async with ctx.step("write"):
            return {
                "due": inputs["due"],
                "weekday": inputs["due"].isoweekday(),
                "invoice": inputs["invoice"],
            }

    return remind


def test_a_date_a_long_text_and_a_file_are_declared_with_their_format() -> None:
    # GIVEN a workflow whose inputs hold a file, a date and a long text
    remind = _remind()

    # WHEN its spec is read
    spec = getattr(remind, WORKFLOW_ATTR)

    # THEN each of the three carries its format, the others keep theirs, and
    #      the declared order is written
    assert isinstance(spec, WorkflowSpec)
    properties = spec.input_schema["properties"]
    assert properties["invoice"] == {
        "type": "string",
        "format": FILE_PATH_FORMAT,
        "description": "the unpaid invoice",
    }
    assert properties["due"] == {"type": "string", "format": DATE_FORMAT}
    assert properties["message"] == {"type": "string", "format": LONG_TEXT_FORMAT}
    assert properties["days"] == {"type": "integer"}
    assert properties["channel"] == {"enum": ["mail", "letter"], "type": "string"}
    assert properties["copies"] == {"type": "array", "items": {"type": "string"}}
    assert spec.input_schema["required"] == ["channel", "days", "due", "invoice"]
    assert spec.input_schema[INPUT_ORDER_KEY] == [
        "invoice",
        "due",
        "message",
        "days",
        "channel",
        "copies",
    ]


@pytest.mark.asyncio
async def test_a_date_reaches_the_workflow_as_a_date_and_a_file_as_its_path() -> None:
    # GIVEN the workflow and the inputs as the runtime hands them over, in JSON
    remind = _remind()
    inputs = {"invoice": "/data/f-12.pdf", "due": "2026-10-03", "days": 7, "channel": "mail"}

    # WHEN it runs
    result = await remind(MockContext(), inputs)

    # THEN the date is a `date`, the file the path it was given, and the
    #      caller's dictionary is left as it was
    assert result == {
        "due": datetime.date(2026, 10, 3),
        "weekday": 6,
        "invoice": "/data/f-12.pdf",
    }
    assert inputs["due"] == "2026-10-03"


@pytest.mark.asyncio
async def test_a_malformed_date_is_refused_with_the_input_named() -> None:
    # GIVEN the workflow and a due date that is not a day
    remind = _remind()

    # WHEN it runs on it
    with pytest.raises(PayloadError) as caught:
        await remind(MockContext(), {"invoice": "/a", "due": "2026-02-30", "days": 1})

    # THEN the refusal names the input
    assert caught.value.field == "due"
    assert "YYYY-MM-DD" in str(caught.value)


@pytest.mark.parametrize(
    "annotation",
    [datetime.datetime, bytes, dict[str, int], list[datetime.date], int | str],
)
def test_an_input_of_an_unknown_type_is_refused_with_a_clear_message(annotation: Any) -> None:
    # GIVEN inputs with one field of a type the launch form does not know
    # REASON(UP013): the field's type is the test parameter, which the class
    # syntax cannot take.
    inputs = TypedDict("Inputs", {"folder": str, "when": annotation})  # type: ignore[misc]  # noqa: UP013

    # WHEN a workflow declares them
    with pytest.raises(AgentConfigError) as caught:
        workflow("odd", "An odd workflow.", inputs=inputs, steps=["a"], action="read")

    # THEN the refusal names the workflow, the input and the types it may take
    message = str(caught.value)
    assert "'odd'" in message
    assert "input 'when'" in message
    assert "apollia.LongText" in message
    assert "apollia.FileInput" in message
    assert "datetime.date" in message


def test_a_format_marker_on_a_type_that_is_not_text_is_refused() -> None:
    # GIVEN an input that puts the long text marker on an integer
    marker = LongText.__metadata__[0]

    class Inputs(TypedDict):
        count: Annotated[int, marker]

    # WHEN a workflow declares it
    with pytest.raises(AgentConfigError) as caught:
        workflow("odd", "An odd workflow.", inputs=Inputs, steps=["a"], action="read")

    # THEN it is refused, naming the input
    assert "input 'count'" in str(caught.value)
