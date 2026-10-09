---
sidebar_position: 5
title: Write a workflow
---

# Write a workflow

A workflow is a sequence of steps you write in Python and whose order you
decide. Each step hands a message to an agent, runs another workflow, asks the
user a question, or runs plain Python. The steps are declared ahead of the code,
so the workflow can be read, started from a form and followed step by step
without anyone opening the module.

Use a workflow when you know the steps. When the steps depend on what the model
finds along the way, give the task to an agent and let it plan:
[Make an agent plan first](/how-to/run-an-orchestrated-agent).

This guide adds a workflow to a package that already declares an agent named
`invoice-clerk`. To create such a package, see
[Write a tool](/how-to/write-a-worker).

## Declare the inputs and the steps

Create `workflows/reminders.py` in the package folder:

```python
"""Workflow of the package invoices: one reminder per late supplier."""

from typing import Annotated, TypedDict

from apollia import LongText, WorkflowCtx, workflow
from apollia.errors import QuestionDeclined


class Reminders(TypedDict):
    month: Annotated[str, "Month to check, for example 2026-09."]
    tone: Annotated[LongText, "How the reminders should sound."]


@workflow(
    "monthly-reminders",
    "Find the late invoices of a month and draft one reminder per supplier.",
    inputs=Reminders,
    steps=["collect", "confirm", "draft"],
    action="read_personal",
)
async def monthly_reminders(ctx: WorkflowCtx, inputs: Reminders) -> dict[str, str]:
    async with ctx.step("collect"):
        late = await ctx.agents.run(
            "invoice-clerk",
            f"List the invoices of {inputs['month']} that are late, one per line.",
        )

    async with ctx.step("confirm"):
        try:
            answer = await ctx.user.ask(
                f"Draft reminders for these invoices?\n{late}",
                options=["Yes", "No"],
            )
        except QuestionDeclined:
            answer = "No"
        if answer != "Yes":
            return {"late": late, "reminders": ""}

    async with ctx.step("draft"):
        reminders = await ctx.agents.run(
            "invoice-clerk",
            f"Draft one reminder per supplier.\nTone: {inputs['tone']}\n{late}",
        )

    return {"late": late, "reminders": reminders}
```

What the decorator takes:

| Argument | Meaning |
| --- | --- |
| `name` | Identifier of the workflow, `[a-z0-9_-]+`, unique in the package. The workflow is addressed as `<package>/<name>`. |
| `description` | What the workflow does, shown on its sheet. |
| `inputs` | A `TypedDict`. Its schema is the launch form of the workflow. |
| `steps` | The step names, in order, with no duplicate. |
| `action` | The most sensitive action family the workflow performs. |

Notes on the contract:

- The function is `async def` and takes exactly two parameters, `(ctx, inputs)`.
- Each step of the body is written `async with ctx.step("name"):`. A name the
  decorator did not declare is refused with an `AgentConfigError`, so the list
  the sheet shows and the code cannot drift apart.
- The runtime records each step as it opens, closes or raises, with its run.
- Return a value that serialises to JSON.
- A declaration that does not hold (a name outside the alphabet, an unknown
  family, an empty step list, an input of an unsupported type) is refused when
  the module is imported.

## Input types

Each input is one of the fields the launch form knows. Any other type is
refused.

| Field | Python type | What the workflow receives |
| --- | --- | --- |
| Text | `str` | A string. |
| Long text | `apollia.LongText` | A string. |
| File | `apollia.FileInput` | The absolute path of the file. |
| Date | `datetime.date` | A `date`. |
| Number | `int`, `float` | A number. |
| Yes or no | `bool` | A boolean. |
| Choice | `Literal[...]` | One of the listed values. |
| List | `list[str]`, `list[int]`, `list[float]` | A list. |

The form follows the order in which the `TypedDict` declares its inputs.

## What a step can do

The `ctx` of a workflow carries five things beyond the logger:

| Call | Does |
| --- | --- |
| `await ctx.llm.complete(messages, ...)` | Calls the model once, on the messages given. With `schema` the answer is constrained and validated; `max_tokens` bounds it; `thinking=False` keeps a model that reasons from spending that bound on its reasoning. Each run may make as many model calls as `[workflows] max_steps` allows. |
| `await ctx.agents.run(agent_id, message)` | Hands `message` to an installed agent as the first turn of a fresh conversation, under the grants of that agent, and returns its final answer. |
| `await ctx.workflows.run(workflow_id, inputs)` | Runs another workflow and returns what it returned. `workflow_id` is `<package>/<workflow>`, or the bare name of a workflow of the same package. Three levels deep at most. |
| `await ctx.user.ask(question, options=..., hint=...)` | Stops the run on a question until the user answers. See [Pause a workflow for human input](/how-to/human-in-the-loop). |
| `ctx.step(name)` | Opens a declared step. |

An agent or a workflow started from a step runs nested in the run: it is stopped
with it and spends from its budget.

`ctx.tools` is `None` in a workflow: the tools are reached through an agent. A
step that needs a tool hands the message to an agent with `ctx.agents.run`, and
that agent works with the tools it was granted. A step that only needs the
model calls `ctx.llm` itself. `ctx.llm` is `None` when the runtime has no model
configured at all.

## Describe, build and run

```bash
python -m apollia describe invoices
apollia-os package build invoices
apollia-os package install invoices-1.2.0.apo
```

`describe` writes the `[[workflows]]` entry of `apollia.toml` from the
decorator: name, module, description, action family, input schema and steps.

The runtime reads the workflows of the installed packages when it starts, so
restart it after the install: run `apollia-os stop`, then start it again. Then
read the workflow as Apollia sees it, and run it:

```bash
apollia-os workflow show invoices/monthly-reminders
apollia-os workflow run invoices/monthly-reminders \
  --input month=2026-09 --input tone="Polite and short." --wait
```

`--input` is repeatable, and each value is read as the type the workflow
declares for it. Without `--wait` the command prints a run id and returns:

```bash
apollia-os workflow status <run-id>
apollia-os workflow logs invoices/monthly-reminders
apollia-os workflow cancel <run-id>
```

`workflow status` shows the run, its result and each of its steps. `workflow
cancel` interrupts the step in progress, and the next ones never start.

## Next steps

- Unit-test the workflow with a mocked context:
  [Test your tools and workflows](/how-to/test-your-agents).
- Start the workflow on a schedule: propose a trigger in the
  [package manifest](/reference/package-manifest#triggers).
- Every `workflow` subcommand is in the [CLI reference](/reference/cli).
