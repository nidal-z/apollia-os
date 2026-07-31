# Choose an autonomy level for an agent

> For any operator who wants to adjust how far an agent can go on its own before asking for a confirmation.

## Prerequisites

- Apollia running and the daemon active.
- At least one agent installed and started.
- Familiarity with the `apollia-os run` command.

## The four levels

| Level | When to use it | Step budget | Automatic verification | Memory injection |
|---|---|---|---|---|
| `assisted` | Exploratory work, unknown task, first run. The agent proposes each action before executing it. | Very short - suitable for validating a prototype. | HITL approval at every step. | No. |
| `supervised` | Standard daily use. The agent moves forward on its own for simple steps and pauses on risky actions. | Moderate - covers most everyday tasks. | Automatic pause before any write or external call. | No. |
| `bounded_autonomous` | Configured automations, recurring pipelines whose behavior you already know. The agent runs to the end unless the budget is exceeded. | Generous - suited to long but bounded workflows. | Verification only when the budget is exceeded. | No. |
| `long_autonomous` | Long-running background tasks, overnight work on large volumes of data. Reserved for proven agents. | Maximum available. | No automatic interruption during the task. | No. |

> The "Memory injection" column is `No` for every level: Apollia never injects memory context automatically, whichever level you choose.

## Steps - Apply a level to one run

The level is set at launch, with `--autonomy`. It applies to that run only and does not modify `apollia.toml`.

```
apollia-os run mon-agent "ma tâche" --autonomy supervised
```

Replace `supervised` with one of the four values: `assisted`, `supervised`, `bounded_autonomous`, `long_autonomous`.

## Steps - Change the global default level

So that every run uses a given level without specifying it each time, edit `apollia.toml`:

```toml
[autonomy]
default_level = "supervised"
```

Restart the daemon after the change so the new default takes effect.

## Verification

After launch, the first log lines of the task show the active level:

```
autonomy.level=supervised agent=mon-agent "autonomy.activated"
```

Open the logs from the agent detail panel or with `apollia-os agent logs mon-agent --follow`.

## If it does not work

- **Unknown value at launch:** if you pass an incorrect value to `--autonomy`, the CLI rejects the command and lists the four valid values (`assisted`, `supervised`, `bounded_autonomous`, `long_autonomous`). Check the spelling, the values are in `snake_case`.
- **The `--autonomy` level is ignored:** check that you are using `apollia-os run`, not `apollia-os start`. The `start` command starts the daemon without running a task; `--autonomy` has no meaning there.
- **The global default does not change:** restart the daemon after modifying `apollia.toml`. A daemon that is already running reads the config at startup only.

> **Technical reference:** [Apollia reference](/reference) - StepBudget, ResilienceLayer, behavior of each autonomy level.
