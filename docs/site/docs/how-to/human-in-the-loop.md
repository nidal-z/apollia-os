---
sidebar_position: 10
title: Pause a workflow for human input
---

# Pause a workflow for human input

Sometimes a run should not decide alone: a rule is missing, a value is
uncertain, a consequential step needs a go-ahead. A workflow stops on a question
with `ctx.user.ask`, waits for the answer, and goes on with it. This guide
covers that call, how the question is answered, and what asks for a human
without any code of yours.

This is the author's side. For the user's view in the desktop application, see
[Approve or reject an action](/operator-help/control/approve-or-reject-an-action).

## Ask a question from a step

Inside a step, `await ctx.user.ask(...)` stops the run until the user answers:

```python
from typing import TypedDict

from apollia import WorkflowCtx, workflow
from apollia.errors import QuestionDeclined


RULES = {"ACME": "suppliers"}


class Routing(TypedDict):
    vendor: str
    amount: float


@workflow(
    "route-invoice",
    "Decide where an invoice is filed, asking when no rule covers the vendor.",
    inputs=Routing,
    steps=["lookup", "decide"],
    action="read",
)
async def route_invoice(ctx: WorkflowCtx, inputs: Routing) -> dict[str, str]:
    async with ctx.step("lookup"):
        folder = RULES.get(inputs["vendor"])

    async with ctx.step("decide"):
        if folder is None:
            try:
                folder = await ctx.user.ask(
                    f"No rule for {inputs['vendor']} ({inputs['amount']:.2f}). "
                    "Where should it be filed?",
                    options=["to-review", "suppliers", "rejected"],
                    hint="Another folder",
                )
            except QuestionDeclined as declined:
                ctx.logger.info("no folder chosen: %s", declined.reason)
                folder = "to-review"

    return {"folder": folder}
```

The signature is `ask(question, *, options=None, hint=None)` and it returns the
answer as a string:

| Argument | Meaning |
| --- | --- |
| `question` | What is asked, as the user reads it. |
| `options` | The proposed choices, 20 at most. |
| `hint` | A placeholder for the free answer. |

The user may always give a free answer instead of a choice. Check the answer
before you branch on it: it is one of the `options` or any text.

## What the wait does

The run waits inside the step that asked. The question is listed with the
decisions waiting for the user, with the run, the workflow and the step it comes
from. The wait lasts until the user answers or the run is stopped:

- the wait spends the wall clock of the budget of the run;
- stopping the run ends the wait;
- a run that is waiting when the runtime stops is failed at the next start, as
  every interrupted run is.

Write a clear question. Its quality drives the quality of the decision.

- Weak: `"Continue?"`
- Better: `"No rule for Acme Corp (1240.00). Where should it be filed?"`

Keep secrets and unnecessary personal data out of a question: it is stored and
shown.

## Answer the question

From the command line, list what is waiting, then answer one item by its id:

```sh
apollia-os approvals pending
apollia-os approvals decide <id> answer --value "suppliers"
apollia-os approvals decide <id> refuse --reason "file it by hand this quarter"
```

`answer` hands `--value` to the run, which goes on: `ask` returns it. `refuse`
declines the question: `ask` raises `QuestionDeclined`, which carries the
`question` and the `reason`.

## When the user declines

`QuestionDeclined` is yours to catch. Caught, the workflow goes on without the
answer, as the example does with a default folder. Left uncaught, the step that
asked fails, and the run with it.

## Test both branches

Under `MockContext`, `ctx.user` is a mock: `answers` maps a question to its
answer, and `declined` maps a question to a reason and makes `ask` raise.
See [Test your tools and workflows](/how-to/test-your-agents).

## What asks without code

Two pauses need no code in a package.

An agent in a conversation asks the user through the native `ask_user` tool.
That is the counterpart of `ctx.user.ask` for an agent: the model decides to
ask, and the answer comes back as the result of the tool.

A sensitive tool call asks for an approval. Every tool declares an action
family, and every family except `read` is sensitive: before such a call runs,
the user is asked, unless they already authorised it. The same
`approvals decide` command answers it, with `once` for this call, `always` for
this call and every later one of the same agent in the same family, or
`refuse`. An organisation policy or a deny rule refuses the call outright, with
no question. This is why the family a tool declares has to say what the tool
really does: see [Write a tool](/how-to/write-a-worker).

<!-- claim:mcp-requires-approval-gates-task-path -->
An MCP server declared as requiring approval (`requires_approval`) adds no pause
of its own when an agent calls one of its tools. What asks before the call is
the rule above: the governance of the run, by action family.

## What a tool cannot do

`ctx.user` is bound in a workflow run only. A tool does not pause: when it lacks
something, it returns what it found or raises a `DomainError`, and the agent
that called it decides whether to ask the user.

`NeedHumanInput`, `ctx.is_resumed` and `ctx.input_response` belong to a task
that pauses and is run again from the top when it resumes. Neither a tool nor a
workflow is resumed that way: raised there, `NeedHumanInput` fails the call or
the run like any other exception.

## Related

- [Autonomy tiers](/explanation/autonomy-tiers) for how approvals fit the
  autonomy model.
- The `approvals` commands in the [CLI reference](/reference/cli).
- [Write a workflow](/how-to/write-a-director) for the steps a question sits in.
