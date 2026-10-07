---
sidebar_position: 9.5
title: Test your tools and workflows
---

# Test your tools and workflows

Apollia gives you two levels of testing. Unit tests exercise a tool or a
workflow in-process with a mocked context, so they run in milliseconds with no
runtime and no model. Suite evals run whole tasks against a live runtime and
score the outcomes. Use unit tests for logic and contracts, evals for
end-to-end behavior and regressions.

This is a how-to. It assumes you have written a tool or a workflow; if not, see
[Write a tool](/how-to/write-a-worker) and
[Write a workflow](/how-to/write-a-director).

## Unit tests with `apollia.testing`

A tool and a workflow are plain `async def` functions, and a test awaits them as
written. `MockContext` is the `ctx` they receive: it mocks every service of the
contract except `ctx.mail`. You configure what the services return, await the
function, and assert on the result and on the calls it made.

## Test a tool

Take the tool of [Write a tool](/how-to/write-a-worker), which counts the pages
of a PDF and raises a typed error when the file is missing:

```python
import pytest

from apollia import DomainError
from apollia.testing import MockContext
from tools.pdf_reader import count_pages


@pytest.mark.asyncio
async def test_count_pages_refuses_a_missing_file():
    # GIVEN a mocked context and a path that does not exist
    ctx = MockContext()

    # WHEN the tool runs
    with pytest.raises(DomainError) as raised:
        await count_pages(ctx, path="/nowhere/report.pdf")

    # THEN it fails with the code a caller can branch on
    assert raised.value.code == "FILE_NOT_FOUND"
```

`@tool` returns the function it decorates, so the test calls it with `ctx` and
the inputs as keyword arguments, the way the runtime does.

## Test a workflow

A workflow hands its steps to agents, to other workflows and to the user. Under
`MockContext` each of those is a mock you configure, and the steps report
nothing. Take the workflow of [Write a workflow](/how-to/write-a-director):

```python
import pytest

from apollia.testing import MockContext
from workflows.reminders import monthly_reminders

INPUTS = {"month": "2026-09", "tone": "Polite and short."}
LATE = "ACME, invoice 42, due 2026-09-10"


@pytest.mark.asyncio
async def test_reminders_are_drafted_once_confirmed():
    # GIVEN an agent that lists one late invoice and a user who says yes
    ctx = MockContext()
    ctx.agents.answers = {"invoice-clerk": LATE}
    ctx.user.answers = {f"Draft reminders for these invoices?\n{LATE}": "Yes"}

    # WHEN the workflow runs
    result = await monthly_reminders(ctx, INPUTS)

    # THEN the agent ran twice, to collect and to draft, and the user was asked
    assert [agent for agent, _ in ctx.agents.run_calls] == ["invoice-clerk"] * 2
    assert len(ctx.user.ask_calls) == 1
    assert result["late"] == LATE


@pytest.mark.asyncio
async def test_nothing_is_drafted_when_the_user_declines():
    # GIVEN a user who declines the question
    ctx = MockContext()
    ctx.agents.answers = {"invoice-clerk": LATE}
    ctx.user.declined = {f"Draft reminders for these invoices?\n{LATE}": None}

    # WHEN the workflow runs
    result = await monthly_reminders(ctx, INPUTS)

    # THEN the run stops after the question, with no reminder
    assert len(ctx.agents.run_calls) == 1
    assert result["reminders"] == ""
```

What each mock takes and records:

| Service | Configure | Read back |
| --- | --- | --- |
| `ctx.agents` | `answers`, a dict of agent id to answer. An agent with no answer returns an empty string. | `run_calls`, a list of `(agent_id, message)`. |
| `ctx.workflows` | `results`, a dict of workflow id to result. A workflow with no result returns `None`. | `run_calls`, a list of `(workflow_id, inputs)`. |
| `ctx.user` | `answers`, a dict of question to answer; `declined`, a dict of question to reason, which makes `ask` raise `QuestionDeclined`. | `ask_calls`, a list of `(question, options, hint)`. |
| `ctx.llm` | `responses`, a FIFO list of `{"content": ...}`; each call pops the next. | `assert_llm_called(ctx, times=...)`. |
| `ctx.tools` | `responses`, a dict of tool name to result. | `assert_tool_called(ctx, name, times=...)`. |
| `ctx.memory` | Records and recalls in memory. | `assert_memory_recorded(ctx, ...)`. |

The decorators behave under a test as they do under the runtime: a step the
decorator did not declare is refused, and a date input given as `YYYY-MM-DD` is
read as a `date`. Run these with `pytest` like any Python tests.

`MockContext` mocks more services than the runtime attaches to a tool or to a
workflow today: `ctx.llm` and `ctx.tools` are `None` there. A test that passes
through them proves the logic of your function, not that the service is
reachable where the function runs.

## Suite evals with `apollia-os eval run`

An eval suite is a TOML file of tasks, each run one or more times against the
running daemon and checked with assertions. Use it to catch regressions and to
score behavior that unit tests cannot, such as answer quality.

The suite format is a `name` and a list of `[[tasks]]`:

```toml
name = "smoke"

[[tasks]]
id = "write-report"
prompt = "Write a one-line report to /tmp/report.txt and print done."
runs = 4
agent = "writer"
assertions = [
  { type = "file_exists", path = "/tmp/report.txt" },
  { type = "regex", on = "result", pattern = "done" },
]
```

- `runs` defaults to 3. `agent` names the target agent (or pass `--agent` on the
  command line as the default for tasks that omit it).
- Assertion `type` is one of `exit_code` (`equals`), `file_exists` (`path`),
  `regex` (`on = "stdout"` or `"result"`, plus `pattern`), and `llm_judge`
  (`rubric`).
- `llm_judge` is parsed but never evaluated. The command builds the
  eval runner without a judge router, and an assertion that cannot be checked
  fails rather than counting as passed, so every `llm_judge` assertion returns
  `llm judge not evaluated: this runner has no judge router` and reddens its
  task. Configuring an LLM backend does not change this. Leave `llm_judge` out
  of any suite you gate on.

Run a suite against a running daemon and re-read a prior result:

```sh
apollia-os eval run ./smoke.toml
apollia-os eval report ./smoke.results.jsonl
```

`eval run` writes one JSONL line per run and prints a summary: success rate,
p50 and p95 wall-clock, and total cost. Step and tool-call counts are reported but
are not reliably surfaced yet, so do not gate on them.

The suite shape above is illustrative; the repository does not ship a ready-made
suite for you to copy. Every flag is in the [CLI reference](/reference/cli), every
field of a suite in the [evaluation suite schema](/reference/eval-suites), and
the service shapes are in the [SDK / ctx contract](/reference/sdk).

## Which to use

- Contract and logic (right payload, right tool, right typed error): unit tests.
- End-to-end behavior, quality, and regression tracking across runs: evals.

Most packages want both: fast unit tests in CI on every change, and an eval
suite run against a runtime before a release.
