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


agent = InvoiceRouter()
```

The constructor is `NeedHumanInput(prompt: str, context: dict | None = None)`. It is
a subclass of `AgentError`, imported from the package root
(`from apollia import NeedHumanInput`).

## What the pause does

When a skill raises `NeedHumanInput`, the dispatcher turns it into a result with
status `input_required` carrying the `prompt` and `context`. The runtime suspends
the task, persists its state, and surfaces it to the operator. The task waits: a
minute or a week, the state stays put until a human answers.

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

Rejecting terminates the task with a rejected status. Approving lets execution
continue. The decision the human returns is a boolean plus an optional reason
string; it is not a free-form answer or a chosen value.

## What your skill receives on resume

Be precise about the contract, so you do not build on something that is not there.
The human's decision is enforced by the runtime: a rejection ends the task, an
approval lets it proceed. A plain `@skill` is not handed the decision or the reason
as an argument when the task resumes, so do not try to read the answer back inside
the skill. In particular, there is no `ctx.memory` or `ctx.profile` key that the
runtime populates with the human's response; reading such a key is not a supported
mechanism.

Design accordingly:

- Use `NeedHumanInput` as a gate, "do not proceed without human approval", rather
  than as a channel to collect data from the human.
- Make the condition that triggers the pause idempotent, so the task behaves
  correctly if the same skill is entered again.
- Resume-aware branching across a suspension is managed by the runtime for chat and
  orchestrated runs; a standalone worker skill does not observe the resume itself.

## `NeedHumanInput` versus `requires_approval`

For the common case of gating a specific sensitive action, prefer the declarative
form: mark the skill with `@skill(..., requires_approval=True)`. The runtime
inserts the approval pause before the skill runs, without your code raising
anything. Reach for `NeedHumanInput` when the decision to pause is dynamic and made
mid-skill.

A third, separate mechanism gates external MCP tools per server or per tool; that
is configured through the `apollia-os mcp` commands, not the agent SDK.

## Related

- [Autonomy tiers](/explanation/autonomy-tiers) for how required approvals fit the
  autonomy model.
- The `task` commands in the [CLI reference](/reference/cli).
- The [SDK / ctx contract](/reference/sdk) for the surfaces a skill uses.
