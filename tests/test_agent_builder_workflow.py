"""Tests for the workflow of the agent workshop, ``draft``.

The workshop is shipped as a package; its one workflow lives in
``agents/system/agent-builder/workflows/draft.py``. These tests run it with a
fake context whose model answers are queued and whose person answers or
declines, and check what the workshop promises: every call is bounded and made
without reasoning, the person is asked three questions at most, an access the
machine does not offer never reaches the sheet, and the mission is laid out
from the parts the model filled.

Run with ``PYTHONPATH=sdk python3 -m pytest tests/test_agent_builder_workflow.py``.
"""

import asyncio
import importlib.util
import json
import sys
import tomllib
from pathlib import Path
from typing import Any

from apollia.errors import QuestionDeclined

_PROJECT_ROOT = Path(__file__).resolve().parent.parent
_PACKAGE = _PROJECT_ROOT / "agents" / "system" / "agent-builder"
_MODULE_PATH = _PACKAGE / "workflows" / "draft.py"

_spec = importlib.util.spec_from_file_location("agent_builder_draft", str(_MODULE_PATH))
assert _spec is not None and _spec.loader is not None
builder = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = builder
_spec.loader.exec_module(builder)


# ---------------------------------------------------------------------------
# Test doubles
# ---------------------------------------------------------------------------


class _FakeLlm:
    """The model: serves queued answers and keeps what each call asked."""

    def __init__(self, answers: list[Any]) -> None:
        self.answers = list(answers)
        self.calls: list[dict[str, Any]] = []

    async def complete(self, messages: list[dict[str, Any]], **settings: Any) -> Any:
        self.calls.append({"messages": messages, **settings})
        return self.answers.pop(0)


class _FakeUser:
    """The person: answers each question from a queue, ``None`` declining it."""

    def __init__(self, answers: list[str | None]) -> None:
        self.answers = list(answers)
        self.asked: list[tuple[str, list[str] | None]] = []

    async def ask(self, question: str, *, options: list[str] | None = None) -> str:
        self.asked.append((question, options))
        answer = self.answers.pop(0)
        if answer is None:
            raise QuestionDeclined(question)
        return answer


class _FakeCtx:
    """The context of a run: the model and the person. The steps are opened
    by the decorator, which reports them to the runtime and to nothing here."""

    def __init__(self, llm: _FakeLlm, user: _FakeUser) -> None:
        self.llm = llm
        self.user = user


OFFER = json.dumps(
    {
        "accesses": [
            {
                "id": "file_read",
                "description": "Read a file.",
                "family": "read",
                "by_default": True,
            },
            {"id": "web_search", "description": "Search the web.\nSecond line.", "family": "read"},
            {"id": "mail_send", "description": "Send a mail.", "family": "send_external"},
            {"id": "file_write", "description": "Write a file.", "family": "write_or_delete"},
        ],
        "sources": [
            {"id": "profile", "label": "the profile of the person"},
            {"id": "folder:quotes", "label": "Quotes"},
        ],
    }
)

PARTS = {
    "name": "  Quote   reader ",
    "role": "You read the quotes of suppliers.",
    "receives": "One or several quotes.",
    "steps": ["Read each quote.", "Compare them."],
    "output": "A table.",
    "rules": ["Never invent a figure."],
    "ask_when": ["A quote cannot be read."],
    "examples": ["Compare these quotes.", "Compare these quotes.", ""],
    "access": ["web_search"],
}


def _run(inputs: dict[str, Any], llm: _FakeLlm, user: _FakeUser) -> dict:
    return asyncio.run(builder.draft(_FakeCtx(llm, user), inputs))


# ---------------------------------------------------------------------------
# The run
# ---------------------------------------------------------------------------


def test_a_form_becomes_a_sheet() -> None:
    # GIVEN a form, a model that asks one question then fills the parts, and
    # a person who answers
    llm = _FakeLlm(
        [{"questions": [{"question": "Which form?", "options": ["A table", "A list"]}]}, PARTS]
    )
    user = _FakeUser(["A table"])
    inputs = {
        "purpose": "Read the quotes of suppliers.",
        "sources": ["folder:quotes", "folder:gone"],
        "language": "en",
        "offer": OFFER,
    }

    # WHEN the workflow runs
    sheet = _run(inputs, llm, user)

    # THEN the question was put with its choices, and the sheet holds the trimmed name, the laid out mission, the
    # distinct examples, the proposed access and the source the machine holds
    assert user.asked == [("Which form?", ["A table", "A list"])]
    assert sheet["name"] == "Quote reader"
    assert sheet["mission"] == (
        "You read the quotes of suppliers.\n\n"
        "## Requests\nOne or several quotes.\n\n"
        "## Method\n1. Read each quote.\n2. Compare them.\n\n"
        "## Expected result\nA table.\n\n"
        "## Rules\n- Never invent a figure.\n\n"
        "## When to stop and ask\n- A quote cannot be read."
    )
    assert sheet["examples"] == ["Compare these quotes."]
    assert sheet["access"] == ["web_search"]
    assert sheet["sources"] == ["folder:quotes"]
    assert sheet["answers"] == [{"question": "Which form?", "answer": "A table"}]


def test_every_call_is_bounded_and_made_without_reasoning() -> None:
    # GIVEN a form and a model that asks nothing
    llm = _FakeLlm([{"questions": []}, PARTS])

    # WHEN the workflow runs
    _run({"purpose": "Read quotes.", "offer": OFFER}, llm, _FakeUser([]))

    # THEN each of the two calls states a token cap, a schema and no reasoning
    assert len(llm.calls) == 2
    for call in llm.calls:
        assert call["thinking"] is False
        assert 0 < call["max_tokens"] <= builder.WRITE_MAX_TOKENS
        assert call["schema"]["type"] == "object"


def test_the_person_is_asked_three_questions_at_most() -> None:
    # GIVEN a model that asks five questions, one of them twice
    asked = [{"question": f"Question {n}?", "options": ["Yes", "No"]} for n in (1, 1, 2, 3, 4)]
    llm = _FakeLlm([{"questions": asked}, PARTS])
    user = _FakeUser(["Yes", None, "  "])

    # WHEN the workflow runs, the person answering, declining, then leaving blank
    sheet = _run({"purpose": "Read quotes.", "offer": OFFER}, llm, user)

    # THEN three distinct questions were put, and only the answer given is kept
    assert [question for question, _ in user.asked] == ["Question 1?", "Question 2?", "Question 3?"]
    assert sheet["answers"] == [{"question": "Question 1?", "answer": "Yes"}]


def test_a_question_with_fewer_than_two_choices_is_asked_open() -> None:
    # GIVEN a model whose question proposes one answer only
    llm = _FakeLlm([{"questions": [{"question": "How?", "options": ["Like this"]}]}, PARTS])
    user = _FakeUser(["My way"])

    # WHEN the workflow runs
    _run({"purpose": "Read quotes.", "offer": OFFER}, llm, user)

    # THEN the question is put without choices
    assert user.asked == [("How?", None)]


def test_the_model_may_only_propose_what_the_person_allowed() -> None:
    # GIVEN a person who allowed sending, and nothing else beyond reading
    llm = _FakeLlm([{"questions": []}, {**PARTS, "access": ["mail_send", "file_write", "made_up"]}])
    inputs = {"purpose": "Answer my mail.", "rights": ["send_external"], "offer": OFFER}

    # WHEN the workflow runs
    sheet = _run(inputs, llm, _FakeUser([]))

    # THEN the schema of the sheet lists the reading and sending accesses, the
    # one every agent is granted left out, and the sheet keeps none beyond them
    enum = llm.calls[1]["schema"]["properties"]["access"]["items"]["enum"]
    assert enum == ["web_search", "mail_send"]
    assert sheet["access"] == ["mail_send"]
    assert "`web_search`: Search the web." in llm.calls[1]["messages"][1]["content"]
    assert "Second line" not in llm.calls[1]["messages"][1]["content"]


def test_without_an_offer_the_sheet_holds_no_access_and_no_source() -> None:
    # GIVEN a form whose offer is not JSON
    llm = _FakeLlm([{"questions": []}, PARTS])
    inputs = {"purpose": "Read quotes.", "sources": ["profile"], "offer": "not json"}

    # WHEN the workflow runs
    sheet = _run(inputs, llm, _FakeUser([]))

    # THEN the schema asks for no access, and the sheet holds none, nor a source
    assert "access" not in llm.calls[1]["schema"]["properties"]
    assert sheet["access"] == []
    assert sheet["sources"] == []


def test_the_sheet_is_written_in_the_language_of_the_person() -> None:
    # GIVEN a form filled in French
    llm = _FakeLlm([{"questions": []}, PARTS])

    # WHEN the workflow runs
    sheet = _run(
        {"purpose": "Lire les devis.", "language": "fr", "offer": OFFER}, llm, _FakeUser([])
    )

    # THEN both calls ask for French and the headings of the mission are French
    assert all("French" in call["messages"][0]["content"] for call in llm.calls)
    assert "## Démarche\n1. Read each quote." in sheet["mission"]


def test_a_part_left_empty_takes_no_heading() -> None:
    # GIVEN parts with a role and rules only
    parts = {"role": "You sort mail.", "rules": ["Never delete."], "steps": []}

    # WHEN the mission is laid out
    mission = builder.mission_of(parts, "en")

    # THEN only the role and the rules appear
    assert mission == "You sort mail.\n\n## Rules\n- Never delete."


# ---------------------------------------------------------------------------
# The manifest
# ---------------------------------------------------------------------------


def test_the_manifest_describes_the_workflow_as_the_code_declares_it() -> None:
    # GIVEN the manifest of the package
    manifest = tomllib.loads((_PACKAGE / "apollia.toml").read_text(encoding="utf-8"))

    # WHEN its workflow is read
    (declared,) = manifest["workflows"]

    # THEN it is the one the code declares, with its steps and its form
    spec = builder.draft.__apollia_workflow__
    assert declared["name"] == spec.name == "draft"
    assert declared["steps"] == list(spec.steps) == ["understand", "ask", "write"]
    assert declared["inputs"]["required"] == ["purpose"]
    assert declared["action"] == "read"
