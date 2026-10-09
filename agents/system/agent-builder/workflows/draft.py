"""The workflow of the agent workshop: turn a filled form into the sheet of an agent.

The person fills a short form on the Agents screen: what the agent is for,
what it must give back, what it must never do, what it may consult and what it
may do. This workflow takes the form and hands back a sheet the person reads
over and saves. Nothing here creates an agent.

A sheet holds when it says four things: what the agent receives, what it gives
back and in what form, what it never does, and when it stops to ask. The
workflow goes after those four, in three steps:

  understand   one bounded call that lists what the form leaves unsaid, as
               questions with proposed answers, three at most
  ask          each question is put to the person, who answers or declines
  write        one bounded call that fills the parts of the sheet; the mission
               is then laid out here, in a fixed order, from those parts

The model never writes an identifier freely: the accesses it may propose are
an ``enum`` of the answer schema, built from what the machine holds. The
sources are the ones the person checked. The runtime checks the sheet again
before the form opens.

The inputs ``offer`` and ``language`` are filled by the application, not by the
person: ``offer`` is what exists on the machine, as JSON.
"""

import json
from typing import Any, NotRequired, TypedDict

from apollia import LongText, WorkflowCtx, workflow
from apollia.errors import QuestionDeclined

# ---------------------------------------------------------------------------
# Bounds
# ---------------------------------------------------------------------------

MAX_QUESTIONS: int = 3
MAX_OPTIONS: int = 4
MAX_EXAMPLES: int = 4
MAX_LIST_ITEMS: int = 8
MAX_NAME_CHARS: int = 60

# Each call is bounded, and made without reasoning: a model that reasons spends
# its cap on the reasoning first and answers nothing, and one that starts
# repeating itself stops here.
UNDERSTAND_MAX_TOKENS: int = 700
WRITE_MAX_TOKENS: int = 1800
TEMPERATURE: float = 0.3

# The family every agent may be offered, whatever the person checked.
READ_FAMILY: str = "read"

# ---------------------------------------------------------------------------
# Wording
# ---------------------------------------------------------------------------

LANGUAGE_NAME: dict[str, str] = {"fr": "French", "en": "English"}

HEADINGS: dict[str, dict[str, str]] = {
    "en": {
        "receives": "Requests",
        "steps": "Method",
        "output": "Expected result",
        "rules": "Rules",
        "ask_when": "When to stop and ask",
    },
    "fr": {
        "receives": "Demandes reçues",
        "steps": "Démarche",
        "output": "Résultat attendu",
        "rules": "Règles",
        "ask_when": "Quand s'arrêter pour demander",
    },
}

UNDERSTAND_SYSTEM: str = (
    "You help a person write the sheet of an AI agent. The person filled a form. "
    "You do not write the sheet yet: you decide what the form leaves unsaid.\n"
    "A sheet holds when four things are known: what the agent receives, what it gives "
    "back and in what form, what it must never do, and when it must stop and ask the "
    "person.\n"
    "List the questions whose answer would change the instructions of the agent, and "
    "only those: {max_questions} at most, none when the form already says enough. "
    "Each question is short, about one thing, and comes with two to four short "
    "proposed answers the person can pick.\n"
    "Never ask where files are stored, which tool or account to use, or which model: "
    "the form settles those. Never ask what the form already says.\n"
    "Write the questions and their answers in {language}."
)

WRITE_SYSTEM: str = (
    "You write the sheet of an AI agent from a form a person filled and from their "
    "answers to a few questions. The agent will read this sheet as its instructions, "
    "so address it as 'you' and write what it must do, not a description of it.\n"
    "Fill each part:\n"
    "- name: two to four words, what a colleague would call this agent.\n"
    "- role: one or two sentences, who the agent is and what it is for.\n"
    "- receives: what a request to this agent contains.\n"
    "- steps: how it proceeds, in order, one action per item.\n"
    "- output: what it gives back and in what form.\n"
    "- rules: what it must always or never do. Keep every rule the person stated, in "
    "their terms.\n"
    "- ask_when: the situations in which it stops and asks the person rather than "
    "guessing.\n"
    "- examples: requests the person would send it, as they would write them.\n"
    "{access_line}"
    "Stay with what the person said. Do not invent a figure, a name, a deadline or a "
    "rule. Write in {language}."
)

ACCESS_LINE: str = (
    "- access: the accesses listed under 'Accesses' that the work of this agent calls "
    "for. The person allowed them: an agent that works on mail needs the mail "
    "accesses, one that looks things up on the web needs the search ones. An agent "
    "that works only on what it is handed needs none.\n"
)

# ---------------------------------------------------------------------------
# Inputs
# ---------------------------------------------------------------------------


class Draft(TypedDict):
    """The form of the agent workshop."""

    purpose: LongText
    result: NotRequired[LongText]
    rules: NotRequired[LongText]
    examples: NotRequired[LongText]
    sources: NotRequired[list[str]]
    rights: NotRequired[list[str]]
    language: NotRequired[str]
    offer: NotRequired[LongText]


# ---------------------------------------------------------------------------
# What the machine holds
# ---------------------------------------------------------------------------


def first_line(value: object) -> str:
    """The first line of a description, empty when there is none."""
    lines = value.strip().splitlines() if isinstance(value, str) else []
    return lines[0].strip() if lines else ""


def read_offer(raw: str | None) -> dict[str, Any]:
    """What exists on the machine, from the JSON the application hands over.

    A text that is not the expected JSON offers nothing: the sheet is then
    written without access or source, and the person adds them in the form.
    """
    empty: dict[str, Any] = {"accesses": [], "sources": []}
    if not raw or not raw.strip():
        return empty
    try:
        parsed = json.loads(raw)
    except ValueError:
        return empty
    if not isinstance(parsed, dict):
        return empty
    accesses = [
        {
            "id": entry["id"],
            "description": first_line(entry.get("description")),
            "family": str(entry.get("family") or ""),
            "by_default": entry.get("by_default") is True,
        }
        for entry in parsed.get("accesses") or []
        if isinstance(entry, dict) and isinstance(entry.get("id"), str) and entry["id"]
    ]
    sources = [
        {"id": entry["id"], "label": str(entry.get("label") or entry["id"])}
        for entry in parsed.get("sources") or []
        if isinstance(entry, dict) and isinstance(entry.get("id"), str) and entry["id"]
    ]
    return {"accesses": accesses, "sources": sources}


def proposable_accesses(offer: dict[str, Any], rights: list[str]) -> list[dict[str, Any]]:
    """List the accesses the model may propose.

    Those of the reading family and of the families the person checked,
    without the ones every new agent is granted, which the runtime adds on
    its own.
    """
    allowed = {READ_FAMILY, *rights}
    return [
        access
        for access in offer["accesses"]
        if access["family"] in allowed and not access["by_default"]
    ]


def checked_sources(offer: dict[str, Any], wanted: list[str]) -> list[dict[str, Any]]:
    """The sources the person checked, among those the machine holds."""
    return [source for source in offer["sources"] if source["id"] in wanted]


# ---------------------------------------------------------------------------
# Prompts and schemas
# ---------------------------------------------------------------------------


def language_of(code: str | None) -> str:
    """The language of the sheet: French or English, English when unknown."""
    return code if code in HEADINGS else "en"


def form_text(inputs: Draft, sources: list[dict[str, Any]]) -> str:
    """The form as the model reads it, the empty fields left out."""
    parts = [("What the agent is for", inputs.get("purpose", ""))]
    parts.append(("What it must give back", inputs.get("result", "")))
    parts.append(("What it must always or never do", inputs.get("rules", "")))
    parts.append(("Requests it will receive", inputs.get("examples", "")))
    parts.append(("What it may consult", ", ".join(source["label"] for source in sources)))
    return "\n\n".join(
        f"## {title}\n{text.strip()}" for title, text in parts if text and text.strip()
    )


def answers_text(answers: list[dict[str, str]]) -> str:
    """The answers of the person, one question after the other."""
    if not answers:
        return ""
    lines = [f"- {entry['question']}\n  {entry['answer']}" for entry in answers]
    return "## Answers of the person\n" + "\n".join(lines)


def accesses_text(accesses: list[dict[str, Any]]) -> str:
    """The accesses the model may propose, one per line."""
    if not accesses:
        return ""
    lines = [f"- `{access['id']}`: {access['description']}" for access in accesses]
    return "## Accesses\n" + "\n".join(lines)


QUESTIONS_SCHEMA: dict[str, Any] = {
    "type": "object",
    "required": ["questions"],
    "properties": {
        "questions": {
            "type": "array",
            "items": {
                "type": "object",
                "required": ["question", "options"],
                "properties": {
                    "question": {"type": "string"},
                    "options": {"type": "array", "items": {"type": "string"}},
                },
            },
        }
    },
}


def sheet_schema(access_ids: list[str]) -> dict[str, Any]:
    """Build the schema of the parts of a sheet.

    The accesses are an ``enum`` of what the model may propose, and are left
    out when there is none.
    """
    text_list = {"type": "array", "items": {"type": "string"}}
    properties: dict[str, Any] = {
        "name": {"type": "string"},
        "role": {"type": "string"},
        "receives": {"type": "string"},
        "steps": text_list,
        "output": {"type": "string"},
        "rules": text_list,
        "ask_when": text_list,
        "examples": text_list,
    }
    required = list(properties)
    if access_ids:
        properties["access"] = {"type": "array", "items": {"type": "string", "enum": access_ids}}
        required.append("access")
    return {"type": "object", "required": required, "properties": properties}


# ---------------------------------------------------------------------------
# Reading what the model answers
# ---------------------------------------------------------------------------


def clean_list(value: object, limit: int) -> list[str]:
    """The texts of a list, trimmed, the empty and repeated ones dropped."""
    kept: list[str] = []
    if not isinstance(value, list):
        return kept
    for item in value:
        if not isinstance(item, str):
            continue
        item = item.strip()
        if item and item not in kept:
            kept.append(item)
    return kept[:limit]


def questions_of(answer: object) -> list[dict[str, Any]]:
    """Read the questions to put to the person.

    Three at most, each with two to four distinct answers. A question with
    fewer than two is asked open.
    """
    raw = answer.get("questions") if isinstance(answer, dict) else None
    questions: list[dict[str, Any]] = []
    for entry in raw if isinstance(raw, list) else []:
        if not isinstance(entry, dict) or not isinstance(entry.get("question"), str):
            continue
        question = entry["question"].strip()
        if not question or any(q["question"] == question for q in questions):
            continue
        options = clean_list(entry.get("options"), MAX_OPTIONS)
        questions.append({"question": question, "options": options if len(options) > 1 else []})
    return questions[:MAX_QUESTIONS]


def mission_of(parts: dict[str, Any], language: str) -> str:
    """Lay the mission out, in a fixed order, from the parts the model filled.

    A part left empty takes no heading.
    """
    headings = HEADINGS[language]

    def text(key: str) -> str:
        value = parts.get(key)
        return value.strip() if isinstance(value, str) else ""

    def bullets(key: str) -> str:
        return "\n".join(f"- {item}" for item in clean_list(parts.get(key), MAX_LIST_ITEMS))

    def numbered(key: str) -> str:
        items = clean_list(parts.get(key), MAX_LIST_ITEMS)
        return "\n".join(f"{rank}. {item}" for rank, item in enumerate(items, start=1))

    blocks = [text("role")]
    for key, body in (
        ("receives", text("receives")),
        ("steps", numbered("steps")),
        ("output", text("output")),
        ("rules", bullets("rules")),
        ("ask_when", bullets("ask_when")),
    ):
        if body:
            blocks.append(f"## {headings[key]}\n{body}")
    return "\n\n".join(block for block in blocks if block)


def sheet_of(
    parts: object,
    language: str,
    proposable: list[str],
    sources: list[str],
    answers: list[dict[str, str]],
) -> dict[str, Any]:
    """Build the sheet handed back.

    A name, a mission, examples, the accesses among the proposable ones, and
    the sources the person checked.
    """
    parts = parts if isinstance(parts, dict) else {}
    name = parts.get("name")
    name = " ".join(name.split())[:MAX_NAME_CHARS] if isinstance(name, str) else ""
    access = [a for a in clean_list(parts.get("access"), len(proposable)) if a in proposable]
    return {
        "name": name,
        "mission": mission_of(parts, language),
        "examples": clean_list(parts.get("examples"), MAX_EXAMPLES),
        "access": access,
        "sources": sources,
        "answers": answers,
    }


# ---------------------------------------------------------------------------
# The workflow
# ---------------------------------------------------------------------------


@workflow(
    "draft",
    "Turn the form of the agent workshop into the sheet of an agent, asking the person "
    "what the form leaves unsaid. Creates nothing: the person reads the sheet over and "
    "saves it.",
    inputs=Draft,
    steps=["understand", "ask", "write"],
    action="read",
)
async def draft(ctx: WorkflowCtx, inputs: Draft) -> dict:
    """Run the three steps on the form and hand the sheet back."""
    language = language_of(inputs.get("language"))
    offer = read_offer(inputs.get("offer"))
    sources = checked_sources(offer, inputs.get("sources") or [])
    proposable = proposable_accesses(offer, inputs.get("rights") or [])
    form = form_text(inputs, sources)

    async with ctx.step("understand"):
        understood = await ctx.llm.complete(
            [
                {
                    "role": "system",
                    "content": UNDERSTAND_SYSTEM.format(
                        max_questions=MAX_QUESTIONS, language=LANGUAGE_NAME[language]
                    ),
                },
                {"role": "user", "content": form},
            ],
            temperature=TEMPERATURE,
            max_tokens=UNDERSTAND_MAX_TOKENS,
            schema=QUESTIONS_SCHEMA,
            thinking=False,
        )
        questions = questions_of(understood)

    answers: list[dict[str, str]] = []
    async with ctx.step("ask"):
        for entry in questions:
            try:
                answer = await ctx.user.ask(entry["question"], options=entry["options"] or None)
            except QuestionDeclined:
                continue
            if answer.strip():
                answers.append({"question": entry["question"], "answer": answer.strip()})

    async with ctx.step("write"):
        access_ids = [access["id"] for access in proposable]
        blocks = [form, answers_text(answers), accesses_text(proposable)]
        parts = await ctx.llm.complete(
            [
                {
                    "role": "system",
                    "content": WRITE_SYSTEM.format(
                        access_line=ACCESS_LINE if access_ids else "",
                        language=LANGUAGE_NAME[language],
                    ),
                },
                {"role": "user", "content": "\n\n".join(block for block in blocks if block)},
            ],
            temperature=TEMPERATURE,
            max_tokens=WRITE_MAX_TOKENS,
            schema=sheet_schema(access_ids),
            thinking=False,
        )
        return sheet_of(parts, language, access_ids, [source["id"] for source in sources], answers)
