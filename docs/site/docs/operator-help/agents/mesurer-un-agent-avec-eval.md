---
title: Measure an agent's performance with apollia-os eval
sidebar_position: 5
---

# Measure an agent's performance with apollia-os eval

> For any operator who wants to quantify the reliability of an agent on a set of reproducible tasks before using it in production.

## Prerequisites

- Apollia running, daemon active.
- The agent to evaluate installed and startable.
- A `suite.toml` file to create (see below).

## Create an evaluation suite

Create a `suite.toml` file in the directory of your choice. Minimal structure:

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

Field by field:

- `name`: readable identifier of the suite, appears in the report.
- `tasks[].id`: identifier of the task, unique within the suite.
- `tasks[].prompt`: text of the task sent to the agent.
- `tasks[].runs`: number of independent runs per task (default: `3`). Use at least 3 to detect non-deterministic answers.
- `tasks[].assertions`: list of checks applied to each run.

Four assertion types exist: `exit_code`, `file_exists`, `regex` and
`llm_judge`. Each takes its own set of fields and rejects the others, so a
mistyped key fails the load rather than skipping the check.

The exact fields per type are in the [evaluation suite schema](/reference/eval-suites),
generated from the parser itself. Two are worth knowing before you write your
first suite: a `regex` matches `stdout` or `result`, never a file, and an
`llm_judge` takes a `rubric` and no expected value.

## Steps - Run the evaluation

```
apollia-os eval run my-suite.toml
```

The command shows a table of results in real time during the run. At the end, it writes a `.results.jsonl` file in the same directory as the suite.

To get machine-readable JSON output (CI integration, scripting):

```
apollia-os eval run my-suite.toml --json
```

## Steps - Read the report

```
apollia-os eval report my-suite.results.jsonl
```

Shows a summary per task: success rate, median time, failed assertions. Use `--json` to get the report in JSON.

## Verification

- The `my-suite.results.jsonl` file is created in the same directory as `suite.toml`.
- The `apollia-os eval run` command exits with exit code `0` if all assertions pass on all runs.
- The `apollia-os eval report` command shows the overall success rate.

## If it does not work

- **"runtime unreachable" at launch:** the Apollia daemon is not started. Run `apollia-os start` then launch the evaluation again.
- **"invalid suite":** check the TOML syntax of your file (brackets, quotes, key names) and make sure that the `type` field of each assertion is one of the four recognized values.
- **The `llm_judge` assertions always fail:** check that the default LLM backend is configured and reachable. The LLM judge uses the same backend as the evaluated agent.
- **The `.results.jsonl` file is not created:** the evaluation failed before producing any results. Run it again with `--json` to see the raw error.

> **Technical reference:** [Apollia reference](/reference) - `eval run` and `eval report` commands, `.results.jsonl` format, CI integration.
