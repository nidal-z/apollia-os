---
sidebar_position: 10
title: Pause an agent for human input
---

# Pause an agent for human input

Sometimes an agent should not decide alone: a sensitive action needs sign-off, a
rule is missing, a value is uncertain. Apollia lets an author pause a running task
and require a human decision before it continues. This guide covers the author-side
primitive, `NeedHumanInput`, and how the pause is resolved.

This is the author's side. For the operator's view of approving or rejecting from
the desktop app, see the operator help. For how autonomy tiers change when
approval is required, see [Autonomy tiers](/explanation/autonomy-tiers).

## Raise `NeedHumanInput` to pause

Inside a skill, raise `NeedHumanInput` when you need a person to weigh in. It takes
a `prompt` shown to the human and an optional `context` dict that is persisted and
restored around the pause.

```python
from apollia import agent, skill, NeedHumanInput
from apollia.types import Ctx


@agent(name="invoice-router", version="0.1.0", description="Route invoices.")
class InvoiceRouter:
    @skill("invoice.route", description="Decide where to file an invoice.")
    async def route(self, vendor: str, amount: float, ctx: Ctx) -> dict:
        folder = await self._lookup_rule(vendor, ctx)
        if folder is None:
            raise NeedHumanInput(
                prompt=f"No rule for {vendor} ({amount:.2f}). Approve filing under 'to-review'?",
                context={"vendor": vendor, "amount": amount},
            )
        return {"folder": folder}
```

The constructor is `NeedHumanInput(prompt: str, context: dict | None = None, *,
payload: dict | None = None)`. It is a subclass of `AgentError`, imported from the
package root (`from apollia import NeedHumanInput`).

## Ask a typed question or an approval

A prompt alone gets a yes or a no back. A `payload` types the pause, so the
operator is offered the answers you expect and your skill receives one of them.
There are two shapes, both declared as `TypedDict` in `apollia.hitl`.

A question has a `genre` among `choix`, `source`, `seuil`, `definition` and
`confirmation`, the `question` itself, and optional `propositions` (each with an
`id` and a `libelle`), `autre` (free text accepted), `portee` and `memoire`:

```python
from apollia import NeedHumanInput
from apollia.hitl import QuestionPayload

question: QuestionPayload = {
    "genre": "choix",
    "question": "Which list should be purged?",
    "propositions": [
        {"id": "leads", "libelle": "Leads"},
        {"id": "clients", "libelle": "Clients"},
    ],
}
raise NeedHumanInput("Which list should be purged?", payload=question)
```

An approval names a `geste`, a `risque` among `low`, `medium` and `critical`, and
optionally `detail` lines and a `delai` in seconds:

```python
from apollia.hitl import ApprovalPayload

approval: ApprovalPayload = {
    "genre": "approbation",
    "geste": "crm/purge",
    "risque": "critical",
    "detail": ["list: clients"],
}
raise NeedHumanInput("Purge the list?", {"list": "clients"}, payload=approval)
```

The runtime checks the payload when the skill pauses. A payload that does not
parse, an unknown `genre`, an empty `question`, two propositions sharing an `id`,
fails the task with the code `INVALID_INPUT_PAYLOAD`. It is never stored or shown.

`AIPResult.input_required(prompt, context, payload=...)` returned from a skill
pauses the task the same way.

## What the pause does

When a skill raises `NeedHumanInput`, the dispatcher turns it into a result with
status `input_required` carrying the `prompt`, `context` and `payload`. The runtime
suspends the task, persists its state, and surfaces it to the operator. The task
waits: a minute or a week, the state stays put until a human answers.

Write a clear prompt. Its quality drives the quality of the decision.

- Weak: `"Continue?"`
- Better: `"No rule for 'Acme Corp' (1240.00). Approve filing under 'to-review'?"`

Keep `context` free of secrets and unnecessary personal data. It is serialized,
stored, and shown in the UI.

## Resolve the pause

An operator sees pending tasks and answers them. From the CLI:

```sh
# List tasks waiting for a human
apollia-os task list --pending-approval

# Approve, or reject with a reason
apollia-os task resume <task-id> --approve
apollia-os task resume <task-id> --reject --reason "file it manually this quarter"

# Review resolved decisions
apollia-os task approvals
```

Over the API, `GET /api/v1/tasks?status=input_required` lists each waiting task
with its `agent`, `skill`, `created_at`, `prompt` and `payload`;
`GET /api/v1/approvals/pending` lists the same pauses with their `agent_name`,
`skill_id`, `prompt`, `context`, `payload` and `suspended_at`. And
`POST /api/v1/tasks/{id}/resume` takes `{"approved", "reason", "answer"}`. The
`answer` is checked against the pending payload:

- a `choix`, `source` or `definition` takes a proposition `id`, or free text when
  `autre` is set;
- a `seuil` takes a number, a `confirmation` a boolean, or a proposition `id`;
- an approval takes no `answer`: `approved` is the decision;
- an approved question needs an `answer`, a declined one does not.

An answer that does not fit is refused with `422` and the code `INVALID_ANSWER`,
and the task stays paused, so the operator can answer again.

A pause without a payload keeps its behaviour: rejecting terminates the task,
approving lets it continue.

## What your skill receives on resume

A resumed task runs your skill again from the top. Two attributes tell it where it
stands:

- `ctx.is_resumed` is `True` on a resumed run;
- `ctx.input_response` is `None` on a first run, and otherwise a dict with
  `approved`, `reason`, `answer`, `payload` (the pause being answered) and
  `context` (the one you passed when pausing).

```python
@skill("list.purge")
async def purge(self, ctx: Ctx) -> dict:
    response = ctx.input_response
    if response is None:
        raise NeedHumanInput("Which list should be purged?", payload=question)
    if response["payload"]["genre"] == "choix":
        chosen = response["answer"]
        raise NeedHumanInput(f"Purge {chosen}?", {"list": chosen}, payload=approval)
    if not response["approved"]:
        return {"purged": None, "reason": response["reason"]}
    return {"purged": response["context"]["list"]}
```

A skill can pause as many times as it needs. Each resume hands back the answer to
the latest pause only, so carry what an earlier pause learned in the `context` of
the next one, as above.

A declined pause that carries a payload resumes the skill with `approved` set to
`False`, so the skill decides what a refusal means. Keep the condition that triggers
a pause idempotent, since the skill is entered again from the top.

To test the resume branch without a runtime, `MockContext.resume_with(...)` puts a
mock context in the state of a resumed task.

## There is no declarative form on `@skill`

Raising `NeedHumanInput` is the only gate an agent opens from its own code. The
decorator takes no approval argument: its parameters are `skill_id`,
`description`, `dangerous` and `examples`, and passing `requires_approval=True`
raises `TypeError: skill() got an unexpected keyword argument
'requires_approval'` at import time, so the agent never loads. `dangerous=True`
is manifest metadata: it marks the skill for inspection tooling and inserts no
pause.

<!-- claim:mcp-requires-approval-gates-task-path -->
An MCP server declared as requiring approval (`requires_approval`), or an MCP tool
the agent manifest lists in `tools_requiring_approval`, gates its calls on the task
path. A call to it from `ctx.tools.call` pauses the task on an approval whose
`geste` is `<server>/<tool>` and whose `detail` lists the arguments. Approved, the
call runs once when the task resumes; declined, `ctx.tools.call` raises
`apollia.errors.ToolApprovalDenied`. A pause raised this way keeps the `context` the
run was resumed with, so a skill resumed from its own pause reads its state again
when it is resumed from this one; on a first run that context is empty. Catch the
`NeedHumanInput` and raise it again with your own `context` to carry something
else. The approval covers that call with those
arguments only. An agent run from an agent chat session goes through the same gate;
the free chat assistant does not, it asks before every tool call it has not been
authorized for.

<!-- claim:mcp-gated-tool-does-not-block-install -->
Declaring such a tool in `tools_required` needs nothing more: the agent installs,
and the approval is held call by call when it runs. The manifest's
`dangerous_tools_allowed` plays no part in it.

## Related

- [Autonomy tiers](/explanation/autonomy-tiers) for how required approvals fit the
  autonomy model.
- The `task` commands in the [CLI reference](/reference/cli).
- The [SDK / ctx contract](/reference/sdk) for the surfaces a skill uses.
