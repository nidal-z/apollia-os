# Measure an agent's performance with apollia-os eval

> For any operator who wants to quantify the reliability of an agent on a set of reproducible tasks before using it in production.

## Prerequisites

- Apollia running, daemon active.
- The agent to evaluate installed and startable.
- A `suite.toml` file to create (see below).

## Create an evaluation suite

Create a `suite.toml` file in the directory of your choice. Minimal structure:

```toml
name = "ma-suite-validation"

[[tasks]]
id        = "resumer-texte"
prompt    = "Résume ce texte en trois phrases : [...]"
runs      = 3

  [[tasks.assertions]]
  type = "exit_code"
  value = 0

  [[tasks.assertions]]
  type = "file_exists"
  path = "output/resume.txt"

  [[tasks.assertions]]
  type = "regex"
  pattern = "\\b(résumé|synthèse)\\b"
  target = "stdout"

  [[tasks.assertions]]
  type = "llm_judge"
  prompt = "La réponse est-elle une synthèse cohérente de trois phrases ? Réponds par OUI ou NON."
  pass_if = "OUI"
```

Field by field:

- `name`: readable identifier of the suite, appears in the report.
- `tasks[].id`: identifier of the task, unique within the suite.
- `tasks[].prompt`: text of the task sent to the agent.
- `tasks[].runs`: number of independent runs per task (default: `3`). Use at least 3 to detect non-deterministic answers.
- `tasks[].assertions`: list of checks applied to each run.

The four assertion types:

| Type | What it checks |
|---|---|
| `exit_code` | The exit code of the run (0 = success). |
| `file_exists` | A file produced by the agent exists at the given path. |
| `regex` | A regular expression matches in `stdout` or in a file. |
| `llm_judge` | A second LLM evaluates the output from a prompt and an expected value. |

## Steps - Run the evaluation

```
apollia-os eval run ma-suite.toml
```

The command shows a table of results in real time during the run. At the end, it writes a `.results.jsonl` file in the same directory as the suite.

To get machine-readable JSON output (CI integration, scripting):

```
apollia-os eval run ma-suite.toml --json
```

## Steps - Read the report

```
apollia-os eval report ma-suite.results.jsonl
```

Shows a summary per task: success rate, median time, failed assertions. Use `--json` to get the report in JSON.

## Verification

- The `ma-suite.results.jsonl` file is created in the same directory as `suite.toml`.
- The `apollia-os eval run` command exits with exit code `0` if all assertions pass on all runs.
- The `apollia-os eval report` command shows the overall success rate.

## If it does not work

- **"runtime unreachable" at launch:** the Apollia daemon is not started. Run `apollia-os start` then launch the evaluation again.
- **"invalid suite":** check the TOML syntax of your file (brackets, quotes, key names) and make sure that the `type` field of each assertion is one of the four recognized values.
- **The `llm_judge` assertions always fail:** check that the default LLM backend is configured and reachable. The LLM judge uses the same backend as the evaluated agent.
- **The `.results.jsonl` file is not created:** the evaluation failed before producing any results. Run it again with `--json` to see the raw error.

> **Technical reference:** [Apollia reference](/reference) - `eval run` and `eval report` commands, `.results.jsonl` format, CI integration.
