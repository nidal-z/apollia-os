---
title: Start an agent or a package
slug: /operator-help/agents/start-an-agent
sidebar_position: 2
---

# Start an agent or a package

> For any operator who has installed an agent (or a package of agents): put it into service so you can talk to it or let its triggers fire.

## Prerequisites

- The agent or the package is installed and visible in the left column of **My Assistants**.
- An AI provider is connected (green dot in the top bar).

## Agent or package: what is the difference in practice?

On the **My Assistants** page, the left column shows two distinct sections:

- **My assistants** - a single agent, identified by a star icon. You start it on its own. This is the format delivered for a simple agent (one `.py` file).
- **My packages** - a coherent set of agents that often work together, identified by a box icon. Starting a package means starting **all its agents and activating its triggers** at once.

You will also find a system agent pinned at the top, **Apollia Chat**: it is always available and requires neither installation nor start.

## Steps - Start a single assistant

1. In the sidebar, open **My Assistants**. The left column lists your assistants under **My assistants · N**.
   ![My Assistants page - left column with the two sections "My assistants" and "My packages" visible](/img/operator-help/agents-demarrer-un-agent-1.png)

2. Find your agent in the list. The dot to the right of its name shows its state: **grey** (stopped), **green** (active), **orange** (degraded).

3. Click the **play button** (▶) at the right of the row. The dot turns green and the button becomes a stop button (■).

4. Click the row (anywhere except the play button) to open the detail panel on the right. There you see its status, its tools, its version and its activity.

5. To talk to it, click **New chat** at the top right of the detail panel. A dedicated conversation opens.

6. To free resources when you no longer need it, click the stop button on the row again. The dot turns grey again.

> Agents marked as **workers** (only called internally by other agents) do not appear in the **My assistants** section - you will find them in the detail of a package.

## Steps - Start a whole package

1. In the sidebar, open **My Assistants**. Scroll the left column down to the **My packages · N** section.

2. The package row shows how many agents and triggers it contains in total and how many are active (for example `0/2 agents · 0/1 triggers` when everything is stopped).

3. Click the **play button** (▶) at the right of the row. Apollia starts every agent of the package and activates their triggers in a single operation. The dot turns green; the counter shows `2/2 agents · 1/1 triggers`.

4. Click the row to open the package detail: there you see the list of the agents it contains, their roles (*director* or *worker*), and the list of the configured triggers (cron, webhook, and so on).
   ![package detail panel - Information, Agents (with director/worker roles) and Triggers sections](/img/operator-help/agents-demarrer-un-agent-2.png)

![package detail panel - Information, Agents (with director/worker roles) and Triggers sections (continued)](/img/operator-help/agents-demarrer-un-agent-2bis.png)

5. If only some of the agents started, the package dot turns **orange** (**partial** status). Click an agent row in the panel to identify the one that is failing, then open its logs.

6. To stop everything at once: click the stop button on the package row again, or use **Stop all** at the top right of the detail panel.

## Special case - Apollia Chat

The system agent **Apollia Chat**, pinned at the top of the list, is **always active**: no start/stop button. Click it to open its configuration panel (personality, tools, model).

## Choose the autonomy level before launching

By default, an agent starts at the `assisted` level: it asks for your approval on every sensitive action. You can choose a different level for a specific run with the `--autonomy` flag:

```
apollia-os run <agent-id> "<your request>" --autonomy <tier>
```

The agent identifier is required. The request text is optional and defaults to
empty, which is rarely what you want.



The four available levels:

| Level | Behavior |
|---|---|
| `assisted` | Default. The plan gate is armed: you approve the plan once before it runs. No verification pass. |
| `supervised` | Plan gate armed, plus one verification pass after the run finishes. |
| `bounded_autonomous` | **The plan gate is bypassed**: the plan runs without your approval. Wider StepBudget, one verification pass at the end. Pass `--plan` to re-arm the gate for a run. |
| `long_autonomous` | Same bypass as above, maximum budget. Reserved for tasks that tolerate running unattended end to end. |

At every level, a filesystem write the runtime judges risky raises its own
approval prompt, independently of the plan gate.

If you omit the flag, the level configured in your preferences applies (`assisted` by default).

> For the detail of the levels and their guarantees, see [Autonomy levels](choisir-un-palier-d-autonomie.md).

## Verification

- **Single assistant** - green dot on the row and in the detail panel. Sending a message in **New chat** triggers a streaming answer.
- **Package** - green dot and a counter such as `N/N agents · M/M triggers`. The triggers (cron, webhook, and so on) are active.

## If it does not work

- **The dot stays orange or red:** open the agent logs from its detail panel (**Logs** link at the bottom) to read the precise error.
- **"AI provider unavailable" error:** check the dot in the top bar and reconnect the provider if needed.
- **Play button greyed out on an agent:** its installation path cannot be found (file moved or deleted). Reinstall it.
- **Play button greyed out on a package:** the source folder of the package is gone (warning icon next to the name). Reinstall the package from its source.
- **Package in "partial" status:** one or more agents did not start. The package detail lists the state of each agent - open the logs of the one that failed.
- **The agent stops too soon:** the StepBudget of the current level has been reached. Raise the level with `--autonomy supervised` or `--autonomy bounded_autonomous` depending on your level of confidence. See [Autonomy levels](choisir-un-palier-d-autonomie.md).
- **The agent starts but does not answer:** see [An agent is stuck](../troubleshooting/un-agent-est-bloque.md).

> **Concept:** [Apollia explanation](/explanation) - understanding the director/worker distinction inside a package and their lifecycle.
