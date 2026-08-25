---
sidebar_position: 9.5
title: Test your agents
---

# Test your agents

Apollia gives you two levels of testing. Unit tests exercise a single skill or
message in-process with a mocked context, so they run in milliseconds with no
daemon and no model. Suite evals run whole tasks against a live daemon and score
the outcomes, including with an LLM judge. Use unit tests for logic and contracts,
evals for end-to-end behavior and regressions.

This is a how-to. It assumes you have written an agent; if not, see
[Write a worker](/how-to/write-a-worker).

## Unit tests with `apollia.testing`

The `apollia.testing` harness runs a skill or a message through the same dispatch
path the runtime uses, but in-process and against a mock `ctx`. `mock(YourAgent)`
returns the agent instance plus a `MockContext` whose 14 service surfaces are all
mocked. You queue what the services return, invoke a skill, and assert on the
result and on the calls the agent made.

The full `Ctx` contract has 15 services: the mail surface (`ctx.mail`) is not
mocked. An agent that uses `ctx.mail` needs an integration test against a live
runtime rather than a unit test with `MockContext`.

Take a skill that summarizes text with the LLM and writes the summary to a file:

```python
from apollia import agent, skill
from apollia.types import Ctx


@agent(name="summarizer", version="0.1.0", description="Summarize and save text.")
class Summarizer:
    @skill("doc.summarize", description="Summarize text and write it to a file.")
    async def summarize(self, text: str, out_path: str, ctx: Ctx) -> dict:
        response = await ctx.llm.complete(
            messages=[{"role": "user", "content": f"Summarize:\n{text}"}],
        )
        await ctx.tools.call("file_write", {"path": out_path, "content": response.content})
        return {"summary": response.content, "path": out_path}
```

A unit test for it, in GIVEN / WHEN / THEN form:

```python
import pytest

from apollia.testing import (
    mock,
    assert_result_completed,
    assert_llm_called,
    assert_tool_called,
)
from summarizer import Summarizer


@pytest.mark.asyncio
async def test_summarize_writes_file():
    # GIVEN a mocked agent with a canned LLM answer and a stubbed file tool
    agent, ctx = mock(Summarizer)
    ctx.llm.responses = [{"content": "A short summary."}]
    ctx.tools.responses = {"file_write": {"ok": True}}

    # WHEN the skill runs
    result = await agent.invoke_skill(
        "doc.summarize", text="a long document", out_path="/tmp/out.txt"
    )

    # THEN it completes, and it used the LLM then the file tool
    assert_result_completed(result, contains="summary")
    assert_llm_called(ctx, times=1)
    assert_tool_called(ctx, "file_write", times=1)
```

Key points of the harness:

- `agent, ctx = mock(YourAgent)` builds the instance and a `MockContext`.
- Drive a skill with `await agent.invoke_skill(skill_id, **kwargs)`, or a
  conversational handler with `await agent.invoke_message(message, history=...)`.
  Both return the same AIPResult dict the runtime produces.
- Queue LLM answers on `ctx.llm.responses` (a FIFO list); each `complete` or
  `chat` pops the next. Stub tools with `ctx.tools.responses` keyed by tool name.
  `ctx.memory` records and recalls in memory.
- Assert on the result with `assert_result_completed(result, contains=...)`,
  `assert_result_failed(result, code=...)`, and
  `assert_result_input_required(result)`. Assert on what the agent did with
  `assert_llm_called`, `assert_tool_called`, `assert_skill_called`,
  `assert_memory_recorded`, `assert_template_rendered`, `assert_emitted_token`,
  and `assert_emitted_thought`.

Because the harness reuses the production dispatch functions, payload validation,
`ctx` injection, return coercion, and typed-error handling behave exactly as they
do under the daemon. Run these with `pytest` like any Python tests.

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
  { type = "llm_judge", rubric = "The report is a single clear sentence." },
]
```

- `runs` defaults to 3. `agent` names the target agent (or pass `--agent` on the
  command line as the default for tasks that omit it).
- Assertion `type` is one of `exit_code` (`equals`), `file_exists` (`path`),
  `regex` (`on = "stdout"` or `"result"`, plus `pattern`), and `llm_judge`
  (`rubric`).
- `llm_judge` grades the output against the rubric using your configured LLM
  router's fast route at temperature 0. If no backend is available the judge is
  skipped rather than failing the run.

Run a suite against a running daemon and re-read a prior result:

```sh
apollia-os eval run ./smoke.toml
apollia-os eval report ./smoke.results.jsonl
```

`eval run` writes one JSONL line per run and prints a summary: success rate,
p50 and p95 wall-clock, and total cost. Step and tool-call counts are reported but
are not reliably surfaced yet, so do not gate on them.

The suite shape above is illustrative; the repository does not ship a ready-made
suite for you to copy. Every flag is in the [CLI reference](/reference/cli), and
the service shapes your agent asserts against are in the
[SDK / ctx contract](/reference/sdk).

## Which to use

- Contract and logic (right payload, right tool, right typed error): unit tests.
- End-to-end behavior, quality, and regression tracking across runs: evals.

Most agents want both: fast unit tests in CI on every change, and an eval suite
run against a daemon before a release.
