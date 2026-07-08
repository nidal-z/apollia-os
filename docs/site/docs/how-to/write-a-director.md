---
sidebar_position: 5
title: Write a director (A2A)
---

# Write a director (A2A)

A director is an agent that answers by orchestrating workers. It exposes a
conversational entry point, hands the workers' skills to the model as tools, and
lets a ReAct loop decide which to call. This guide builds a document assistant
that drives the PDF worker from [Write a worker](/how-to/write-a-worker).

## Prerequisites

- The `pdf-quickstart` worker installed and active, exposing `pdf.read_text` and
  `pdf.count_pages`. Verify with `apollia-os a2a skills`.
- An LLM backend configured (`apollia-os llm status`).

## Expose workers as tools, then react

The director is an `@on_message` agent. Inside the handler, call the free
function `apollia.react(...)`: give it a system prompt, the user message, and a
list of tools. Turn each worker skill into a tool with
`await ctx.a2a.skill_as_tool(skill_id)`, which is asynchronous and resolves the
skill's schema at call time.

Create `document_director.py`:

```python
"""Document assistant: drives the PDF worker through apollia.react."""

from apollia import agent, on_message, react
from apollia.types import Ctx, Message


SYSTEM_PROMPT = """\
You are a document assistant. Answer the user's question about a local PDF
by using the available tools:

- `pdf.read_text`: extract the text of a PDF, page by page.
- `pdf.count_pages`: count the pages of a PDF.

Reason step by step. When you have enough information, write a concise answer
and mention the file you inspected.
"""


@agent(
    name="document-director",
    version="0.1.0",
    description="Answers questions about local PDF files by orchestrating a worker.",
)
class DocumentDirector:
    @on_message
    async def chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> str:
        return await react(
            ctx,
            system=SYSTEM_PROMPT,
            user=message,
            tools=[
                await ctx.a2a.skill_as_tool("pdf.read_text"),
                await ctx.a2a.skill_as_tool("pdf.count_pages"),
            ],
            max_steps=8,
        )


agent = DocumentDirector()
```

`react` is a free function, not a `ctx` method: you pass `ctx` as its first
argument. It runs the observe, reason, act loop and returns the model's final
answer as a string. Its full signature (including `temperature` and `max_steps`,
which defaults to 15) is in the [SDK / ctx contract](/reference/sdk); the
`skill_as_tool` and other A2A methods are on [`ctx.a2a`](/reference/sdk/a2a).

If the director references a skill that no active worker exposes, it fails fast
at run time with an unknown-skill error. Install and enable the worker first.

## Install and run

```bash
apollia-os inspect document_director.py
apollia-os agent install ./document_director.py
apollia-os agent enable document-director
apollia-os run document-director "How many pages are in /tmp/report.pdf, and what is it about?"
```

The model decides to call `pdf.count_pages`, then `pdf.read_text`, then writes
its answer.

## Variation: build the tool list dynamically

Instead of naming skills one by one, discover them and filter by namespace:

```python
all_skills = await ctx.a2a.list_skills()
tools = [
    await ctx.a2a.skill_as_tool(s["skill_id"])
    for s in all_skills
    if s["skill_id"].startswith("pdf.")
]
```

Keep the tool list small. Exposing more than about ten tools at once makes the
model's choices less reliable.

## Variation: call a worker directly

When you already know which skill to call and do not need the model to decide,
invoke it directly with `ctx.a2a.invoke`. It returns the full A2A envelope, so
unwrap the skill's dict with `a2a_result_data`:

```python
from apollia.utils import a2a_result_data

envelope = await ctx.a2a.invoke("pdf.count_pages", {"path": "/tmp/report.pdf"})
data = a2a_result_data(envelope)
page_count = data["page_count"]
```

Use `react` when the sequence of calls depends on intermediate results; use
`invoke` for a fixed, known step.

## Variation: handle a stalled loop

`react` raises a `DomainError` with code `REACT_MAX_STEPS` if it runs out of
steps. Catch it to degrade gracefully:

```python
from apollia import DomainError

try:
    return await react(ctx, system=SYSTEM_PROMPT, user=message, tools=tools, max_steps=5)
except DomainError as exc:
    if "REACT_MAX_STEPS" in exc.code:
        return "I could not finish the analysis in time. Could you narrow the question?"
    raise
```

## Next steps

- Let the runtime plan and execute multi-step work for you, with no ReAct loop
  of your own: [Run an orchestrated agent](/how-to/run-an-orchestrated-agent).
- Put it all together across several workers:
  [Build a multi-agent assistant](/tutorials/build-a-multi-agent-assistant).
