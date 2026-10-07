---
sidebar_position: 6
title: Make an agent plan first
---

# Make an agent plan first

An agent can work in two ways. Step by step, it chooses each tool as it goes.
Plan first, it writes the list of steps of the task before acting, then runs
them. Planning suits a task that takes several tool calls whose order matters;
it costs one more model call before anything happens. This guide sets the
planning mode of an agent, in its package and on your machine.

You write no control flow for this: the agent is data, and planning is one of
its settings. When you know the steps yourself, write them instead:
[Write a workflow](/how-to/write-a-director).

## What plan first does

The agent writes a plan: a list of steps, each a tool call or a model call, with
the steps it depends on. The runtime then:

1. checks the plan: unique steps, known dependencies, no cycle, and no more
   steps than the budget of the run allows;
2. runs it level by level, the independent read-only steps of a level together;
3. replans a step that failed, and a result the verification refuses, at the
   autonomy tiers that verify;
4. keeps the plan in the plan cache, so the same task of the same agent runs
   again without planning.

The steps call tools the way the step-by-step loop does: only the tools granted
to the agent, through the same approvals. Every model call of a step goes to the
model of the agent.

## The three modes

| `planning` | The agent plans |
| --- | --- |
| `never` | Never. It acts step by step. |
| `auto` | When the task reads as a multi-step one. This is the default. |
| `always` | Every task. |

In `auto`, the decision involves no model call: the request and the agent are
scored, and the task is planned when the score reads it as several steps.

## Set it in the package

The author of a package recommends a mode in the defaults of the agent:

```toml
[[agents]]
id = "briefing"
name = "Briefing"
mission = """
Given a topic, build a one-page briefing: the context in two or three
sentences, five to seven key facts with their source, and three open
questions. If you cannot verify a fact, say so in the briefing.
"""
examples = ["Give me a briefing on the new accounting rules."]

[agents.defaults]
access = ["web_search", "web_read"]
planning = "always"
autonomy = "supervised"
```

`access` lists the tools the plan may use. `web_search` and `web_read` are
native tools; the full set is in the [tool reference](/reference/native-tools).
The other keys of the table are in the
[package manifest reference](/reference/package-manifest).

## Change it on your machine

A value of the package is a default. Override it for one agent, and a package
update keeps your choice:

```bash
apollia-os agent set briefing planning always
apollia-os agent show briefing
apollia-os agent reset briefing --field planning
```

`agent show` prints the definition as resolved: the defaults of the package,
then your overrides. `agent reset --field planning` goes back to the default of
the package.

## Run it

```bash
apollia-os run briefing "Give me a briefing on the new accounting rules."
```

The agent plans, runs the plan and answers as it would step by step. A plan
that cannot be read, a step that still fails after its replans, or a spent
budget fails the run.

## The budget bounds the plan

A plan cannot hold more steps than the budget of the run, and the runtime
enforces that budget whatever the agent does. The budget comes from the
autonomy tier of the agent, the `autonomy` key above or
`apollia-os agent set briefing autonomy supervised`. See
[Autonomy tiers](/explanation/autonomy-tiers) for what each tier allows.

## Approving a plan

Plan first has no approval gate of its own: the plan runs once it is checked. In
a conversation, reviewing a plan before it runs is the plan mode of the
conversation, described in
[Approve or reject an action](/operator-help/control/approve-or-reject-an-action).
When a conversation is in plan mode, the agent does not plan first on top of it.

## The plan cache

A plan is cached under the agent, the version of its definition, its mission and
the task. A new mission, or a new version of the package, therefore plans again.
To inspect or empty the cache:

```bash
apollia-os plan cache stats
apollia-os plan cache evict --max-age-days 7
apollia-os plan cache clear
```

## Related

- [The plan and orchestrated execution model](/explanation/the-plan-model) for
  the model behind a plan.
- [Write a workflow](/how-to/write-a-director) when the steps are yours to
  write.
- The `agent` and `plan` commands in the [CLI reference](/reference/cli).
