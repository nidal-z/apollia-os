---
sidebar_position: 6
title: Run an orchestrated agent
---

# Run an orchestrated agent

An orchestrated agent hands the whole loop to the runtime. You provide a system
prompt and declare the tools it may use; ORIA, Apollia's orchestration engine,
plans the steps, executes them, observes the results, and replans as needed. You
write almost no control flow. This guide builds a briefing agent that researches
a topic and returns a one-page summary.

Use this pattern when the work is multi-step and driven by natural-language
intent. When you want to keep the loop in your own hands, use a director instead
([Write a director](/how-to/write-a-director)).

## Prerequisites

- An LLM backend configured. Orchestrated mode fails immediately without one.
  Check with `apollia-os llm status`.

## Stack `@agent` over `@orchestrated`

`@orchestrated(system_prompt=...)` is the entry point: it is mutually exclusive
with `@skill` and `@on_message`. Declare the native tools the agent may use with
`tools_required=(...)`; ORIA fails fast at boot if a required tool is missing.

Create `briefing_agent.py`:

```python
"""Briefing assistant. ORIA plans and executes the steps."""

from apollia import agent, orchestrated


SYSTEM_PROMPT = """\
You are a briefing assistant. Given a topic, build a one-page briefing that
covers:

1. Context: two to three sentences on why the topic matters now.
2. Key facts: five to seven bullet points, each with a source.
3. Open questions: three questions a decision-maker would ask.

Use the available tools to gather facts:
- `web_search` to find recent information.
- `web_read` to retrieve a full article when needed.

Stay grounded. If you cannot verify a fact, say so explicitly in the briefing.
"""


@agent(
    name="briefing",
    version="0.1.0",
    description="Synthesize a one-page briefing on any topic.",
    tools_required=("web_search", "web_read"),
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class Briefing:
    pass
```

That is a complete agent. It declares no methods: ORIA runs the loop from the
system prompt and, by default, concatenates the text of each plan step into the
final answer. The tool names `web_search` and `web_read` are Apollia's native
tools; browse the full set in the [tool reference](/reference/native-tools).

## Install and run

```bash
apollia-os inspect briefing_agent.py
apollia-os agent install ./briefing_agent.py
apollia-os agent enable briefing
apollia-os run briefing "Give me a briefing on Microsoft's Permanent Beta culture."
```

ORIA observes the prompt and available tools, reasons out a three to six step
plan, executes it (searching and reading as needed), and returns the assembled
briefing.

## Shape the output with `on_plan_complete`

To post-process the step outputs yourself, define an `on_plan_complete` hook. The
runtime calls it with the step results and the context, and expects a string
back:

```python
from apollia.types import Ctx


@agent(
    name="briefing",
    version="0.1.0",
    description="Synthesize a one-page briefing on any topic.",
    tools_required=("web_search", "web_read"),
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class Briefing:
    async def on_plan_complete(
        self,
        step_results: dict[str, str],
        ctx: Ctx,
    ) -> str:
        sections = []
        for step_id, text in step_results.items():
            if "facts" in step_id and text:
                sections.append(f"- {text}")
        return "## Key facts\n\n" + "\n".join(sections)
```

`step_results` maps each step id (for example `step_3_facts`) to that step's text
output. If you omit the hook, ORIA concatenates the step texts in order.

## Variation: a custom step budget

Every run is bounded by a step budget the runtime enforces and never lets an
agent bypass. Override the defaults on `@agent`:

```python
@agent(
    name="briefing",
    version="0.1.0",
    description="Synthesize a one-page briefing on any topic.",
    tools_required=("web_search", "web_read"),
    step_budget={"max_steps": 25, "max_tool_calls": 40, "wall_clock_secs": 600},
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class Briefing:
    pass
```

An oversized budget is clamped to the runtime ceiling. See
[`ctx.budget`](/reference/sdk/budget) for reading the remaining budget from
inside a run.

## `@orchestrated` versus `apollia.react`

Both drive a multi-step loop. They differ in who holds control.

| Criterion | `@orchestrated` | `apollia.react` |
|---|---|---|
| Who drives the loop | ORIA runtime | You, in `@on_message` |
| Best for | Autonomous multi-step, natural-language intent | A known workflow you want to control |
| Code volume | Very short | Short |
| Pre and post processing | Limited to the `on_plan_complete` hook | Free Python before and after `react` |
| Conditional branches | Hard to express | Natural |
| Conversational mode | No free-chat mode | Yes, through `@on_message` |

## Configure the backend

Orchestrated runs need an LLM backend. Set a single default:

```bash
apollia-os llm backends set-default <name>
```

For multiple backends routed by task, configure `[llm.routing]` in your Apollia
config. See the [configuration reference](/reference/configuration) and the
[`llm` CLI commands](/reference/cli).

## Next steps

- Combine orchestration and workers into a full assistant:
  [Build a multi-agent assistant](/tutorials/build-a-multi-agent-assistant).
