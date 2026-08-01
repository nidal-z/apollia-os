# Choose an autonomy level for an agent

> For any operator who wants to adjust how far an agent can go on its own before asking for a confirmation.

## Prerequisites

- Apollia running and the daemon active.
- At least one agent installed and started.
- Familiarity with the `apollia-os run` command.

## The four levels

| Level | When to use it | Step budget | Automatic verification | Memory injection |
|---|---|---|---|---|
| `assisted` | Exploratory work, unknown task, first run. | Very short - suitable for validating a prototype. | None. The plan gate is armed, so you approve the plan before it runs. | No. |
| `supervised` | Standard daily use. | Moderate - covers most everyday tasks. | One verification pass after the run finishes. | No. |
| `bounded_autonomous` | Configured automations, recurring pipelines whose behavior you already know. | Generous - suited to long but bounded workflows. | One verification pass after the run finishes. | No. |
| `long_autonomous` | Long-running background tasks, overnight work on large volumes of data. Reserved for proven agents. | Maximum available. | One verification pass after the run finishes. | No, for an agent. |

Two things the table would let you assume, and should not.

**Approval is not per step.** No level pauses before each action. What `assisted`
and `supervised` arm is the **plan gate**: you see the plan and approve it once,
before execution. `bounded_autonomous` and `long_autonomous` bypass that gate
entirely, so the plan runs without you seeing it, unless you pass `--plan` to
re-arm it for that run. Separately, and at every level, a filesystem write the
runtime judges risky raises its own approval prompt.

**Verification runs once, at the end.** It is a single post-run pass, not a
check between steps, and `assisted` does not run it at all.

> The "Memory injection" column is `No` for every level **on an agent run**, and
> that is the guarantee that matters: nothing injects memory into an agent's
> prompt. The built-in conversational assistant is a different path, and at the
> `long_autonomous` level it does receive a user-persona brief. An agent cannot
> reach that code.

## Steps - Apply a level to one run

The level is set at launch, with `--autonomy`. It applies to that run only and does not modify `apollia.toml`.

```
apollia-os run my-agent "my task" --autonomy supervised
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
autonomy.level=supervised agent=my-agent "autonomy.activated"
```

Open the logs from the agent detail panel or with `apollia-os agent logs my-agent --follow`.

## If it does not work

- **Unknown value at launch:** if you pass an incorrect value to `--autonomy`, the CLI rejects the command and lists the four valid values (`assisted`, `supervised`, `bounded_autonomous`, `long_autonomous`). Check the spelling, the values are in `snake_case`.
- **The `--autonomy` level is ignored:** check that you are using `apollia-os run`, not `apollia-os start`. The `start` command starts the daemon without running a task; `--autonomy` has no meaning there.
- **The global default does not change:** restart the daemon after modifying `apollia.toml`. A daemon that is already running reads the config at startup only.

> **Technical reference:** [Apollia reference](/reference) - StepBudget, ResilienceLayer, behavior of each autonomy level.
