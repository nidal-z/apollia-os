---
sidebar_position: 7
title: Evaluation suite schema
---

# Evaluation suite schema

An evaluation suite is a TOML file you write by hand. It names a set of tasks;
each task carries a prompt, a run count, an optional target agent, and a list of
typed assertions that decide pass or fail. `apollia-os eval run` parses it,
executes it, and exits non-zero if any assertion fails on any run.

For the operator walkthrough, with the interface steps and how to read a report,
see [Measure an agent with eval](/operator-help/agents/measure-an-agent-with-eval).

The tables below are generated from the Rust types that parse the file, so they
cannot drift from what the parser accepts. A field absent from them is a field
the parser rejects.

<!-- BEGIN GENERATED: eval-schema -->

### The suite

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | `String` | **required** | Human-readable suite name, surfaced in reports. |
| `tasks` | `Vec<EvalTask>` | optional | Tasks to evaluate. Defaults to empty when the `tasks` array is absent. |

### A task

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `id` | `String` | **required** | Stable identifier for the task, used as the report row key. |
| `prompt` | `String` | **required** | The instruction submitted to the agent. |
| `runs` | `u32` | optional | Number of times the task is executed. Defaults to 3. |
| `agent` | `Option<String>` | optional | Target agent identifier. `None` defers the choice to the caller. |
| `assertions` | `Vec<Assertion>` | optional | Typed pass/fail assertions evaluated on each run. |

### Assertions

Each entry under `[[tasks.assertions]]` carries a `type` key that selects
the shape. The fields listed are the ones that shape accepts, and no others.

| `type` | Its fields | What it checks |
| --- | --- | --- |
| `exit_code` | `equals` | Passes when the run exit code equals `equals`. |
| `file_exists` | `path` | Passes when a file exists at `path` after the run. |
| `regex` | `on`, `pattern` | Passes when `pattern` matches the selected output channel. |
| `llm_judge` | `rubric` | Passes when an LLM judge rates the output against `rubric` as a pass. |

### `on`, the channel a `regex` assertion matches against

| Value | Meaning |
| --- | --- |
| `stdout` | The streamed stdout of the run. |
| `result` | The final result text of the run. |
<!-- END GENERATED: eval-schema -->

## A complete example

```toml
name = "my-validation-suite"

[[tasks]]
id        = "summarize-text"
prompt    = "Summarize this text in three sentences: [...]"
runs      = 3

  [[tasks.assertions]]
  type = "exit_code"
  equals = 0

  [[tasks.assertions]]
  type = "file_exists"
  path = "output/summary.txt"

  [[tasks.assertions]]
  type = "regex"
  pattern = "\\b(summary|synthesis)\\b"
  on = "stdout"

  [[tasks.assertions]]
  type = "llm_judge"
  rubric = "The answer must be a coherent three-sentence summary of the source text."
```

## What the parser refuses

Getting a field name wrong fails the load rather than silently skipping the
check, which is the behaviour you want from a test harness. If `apollia-os eval
run` reports an invalid suite, compare your assertion keys against the tables
above before looking anywhere else.

`llm_judge` takes a rubric and nothing else. There is no expected value to
supply: the rubric is the whole criterion, and the judge answers against it.

`apollia-os eval run` builds its runner without a judge, so every `llm_judge`
assertion fails today, with the reason "llm judge not evaluated: this runner has
no judge router". That is deliberate on the harness side, an assertion nothing
checked is never counted as passing, and it is not a backend problem:
configuring one changes nothing here. Until the command wires a judge, write
your suites on `exit_code`, `file_exists` and `regex`.
