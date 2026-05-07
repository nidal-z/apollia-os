# Apollia Python SDK

Python toolkit for building agents that run on [Apollia OS](https://github.com/nidal-z/apollia-os) — a Rust runtime for sovereign, local-first AI agent execution.

The SDK provides base classes, type stubs, parsing/formatting utilities, testing mocks, and a scaffolding CLI so you can build, test, and ship agents without touching Rust.

## Installation

**Prerequisites:** Python >= 3.10

```bash
# From the repository root
pip install -e ./sdk

# With development dependencies (pytest, mypy)
pip install -e "./sdk[dev]"
```

Verify the installation:

```bash
python -c "import apollia; print(apollia.__version__)"
# 0.1.0
```

## Quickstart

Every Apollia agent is a Python class that implements `manifest()` and `run()`. The runtime injects a `ctx` (RuntimeContext) with optional capabilities: `ctx.tools`, `ctx.llm`, `ctx.memory`.

### ReAct Agent (default)

A ReAct agent uses an LLM to reason, then acts by calling tools in a loop.

```python
from apollia.agents import BaseReActAgent, AIPResult
from apollia.utils.hitl import resume_pending_tool


class CodeReviewer(BaseReActAgent):
    SYSTEM_PROMPT = "You are a code reviewer. Analyze files and suggest improvements."
    MAX_STEPS = 8

    def manifest(self):
        return {
            "name": "code-reviewer",
            "version": "1.0.0",
            "description": "Reviews code quality",
            "tools_required": ["bash_executor", "file_io"],
            "execution_mode": "direct",
            "dangerous_tools_allowed": False,
        }

    async def run(self, task, ctx):
        user_msg = task["input"]["parts"][0]["text"]
        pending = resume_pending_tool(task)
        result = await self.react(task, ctx, user_msg, pending_tool=pending)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)
```

Key points:
- `SYSTEM_PROMPT` configures the LLM persona.
- `MAX_STEPS` caps the reason-act loop (the Rust StepBudget is the hard limit).
- `react()` returns either a `str` (final answer) or a `dict` (AIPResult for HITL/failure).

### Conversational Agent

A pure dialogue agent — no tools, just LLM conversation with history.

```python
from apollia.agents import ConversationalAgent


class Greeter(ConversationalAgent):
    SYSTEM_PROMPT = "You are a friendly greeter who welcomes new users."
    MAX_TURNS = 20
    TEMPERATURE = 0.7

    def manifest(self):
        return {
            "name": "greeter",
            "version": "0.1.0",
            "execution_mode": "direct",
            "tools_required": [],
        }
```

The base class handles `run()` automatically: it extracts the user message, calls `converse()`, and returns `AIPResult.completed()`. Override `on_response()` to post-process the LLM output.

### Orchestrated Agent

In orchestrated mode, ORIA (the reasoning engine) generates and executes a plan. The agent only provides metadata and optional post-processing — `run()` is never called.

```python
from apollia.agents import OrchestratedAgent


class DataAnalyzer(OrchestratedAgent):
    def manifest(self):
        return {
            "name": "data-analyzer",
            "version": "0.1.0",
            "execution_mode": "orchestrated",
            "system_prompt": "Analyze data files using available tools.",
            "tools_required": ["bash_executor", "file_io"],
        }

    def on_plan_complete(self, step_results):
        summary = self.format_step_results(step_results)
        return {"text": f"Analysis complete:\n{summary}"}
```

## API Reference

### `apollia.types`

| Class | Description |
| --- | --- |
| `AIPResult` | Dataclass returned by `run()`. Factory methods: `.completed(text)`, `.failed(code, message)`, `.input_required(prompt, context)`. Serializes via `.to_dict()`. |

### `apollia.agents`

| Class | Description |
| --- | --- |
| `BaseReActAgent` | Abstract base for ReAct agents. Provides `react()` loop with HITL support, history persistence, and tool calling. Override `manifest()` and `run()`. |
| `ConversationalAgent` | Abstract base for dialogue agents. Provides `converse()` with history and memory. Override `manifest()` and optionally `on_response()`. |
| `OrchestratedAgent` | Abstract base for ORIA-piloted agents. Override `manifest()` and optionally `on_plan_complete()`. |
| `AIPResult` (from `react.py`) | Dict-based result factory used inside `BaseReActAgent.run()`. Methods: `.completed(text)`, `.failed(code, message)`, `.input_required(prompt, context)`. |

### `apollia.stubs`

Type stubs for IDE autocompletion and mypy validation. These mirror the real PyO3 classes injected at runtime.

| Class | Description |
| --- | --- |
| `RuntimeContext` | Execution context with `.tools`, `.llm`, `.memory` properties (each `None` when unavailable). Also provides `.send()` / `.receive()` for inter-agent messaging. |
| `ToolProxy` | Tool registry proxy. Methods: `call()`, `list_tools()`, `tool_call_count()`, `describe()`. |
| `LlmProxy` | LLM router proxy. Methods: `chat()`, `complete()`, `stream()`, `run_tools()`. Property: `default_backend`. |
| `LlmResponse` | LLM call result. Properties: `content`, `latency_ms`, `usage`. |
| `TokenUsage` | Token statistics. Properties: `prompt_tokens`, `completion_tokens`, `cost_usd`. |
| `MemoryInterface` | Agent memory proxy. Methods: `record()`, `remember()`, `recall()`, `search()`, `forget()`. |

### `apollia.utils.parsing`

| Function | Description |
| --- | --- |
| `extract_json(content)` | Extract the first JSON object from text (tries raw parse, fenced block, outermost braces). Returns `{}` on failure. |
| `extract_code_block(content, language="")` | Extract the first fenced code block. Optional language filter. Returns `None` if not found. |
| `extract_xml_tag(content, tag)` | Extract content between `<tag>...</tag>`. Returns `None` if not found. |
| `safe_json_loads(text, default=None)` | Parse JSON without raising — returns `default` on failure. |
| `truncate(text, max_chars=2000, marker="...")` | Truncate text with a marker. Unicode-safe. |
| `validate_action(data)` | Validate a ReAct action dict structure. Raises `ActionParseError`. |

### `apollia.utils.formatting`

| Function | Description |
| --- | --- |
| `format_as_text(data)` | Convert any value to readable plain text. |
| `format_as_markdown(data)` | Convert dict or list-of-dicts to Markdown tables. |
| `format_as_json(data, indent=2)` | Serialize to indented JSON (never raises — uses `str()` fallback). |
| `aip_result_text(result)` | Extract concatenated text from an AIPResult dict's `parts`. |
| `parts_to_text(parts)` | Join all text parts from an AIP parts list. |

### `apollia.utils.hitl`

| Function | Description |
| --- | --- |
| `resume_pending_tool(task)` | Extract the pending tool call from a HITL-resumed task. Returns `{"tool": name, "args": dict}` or `None`. |

### `apollia.tools.schemas`

The runtime tool registry is the single source of truth for tool descriptors.
`BaseReActAgent.react()` builds its system prompt by calling
`ctx.tools.describe(name)` for every allowed tool, so the LLM always sees the
same schema the runtime enforces at dispatch.

| Symbol | Description |
| --- | --- |
| `build_tools_block_from_ctx(ctx, tool_names)` | **Preferred.** Async builder that pulls live descriptors from the runtime. Falls back to the legacy mirror when `ctx.tools` is unavailable. |
| `render_descriptor(name, descriptor)` | Render one tool's prompt block from a descriptor dict (the result of `ctx.tools.describe()`). |
| `NATIVE_TOOL_SCHEMAS` | Legacy offline mirror of the Rust descriptors. Used as a fallback in tests, dry-runs and contexts without a runtime. Best-effort, not authoritative. |
| `build_tools_block(tool_names)` | Synchronous builder over the legacy mirror. Keep for offline use only. |
| `describe_tool(tool_name)` | Synchronous renderer over the legacy mirror. |

### `apollia.testing`

| Class/Function | Description |
| --- | --- |
| `MockContext` | Factory for test contexts. Use `MockContext.create(tools=..., llm_responses=..., memory=True)`. |
| `MockToolProxy` | Mock tool registry. Records calls, returns pre-configured responses. |
| `MockLlmProxy` | Mock LLM. Consumes responses in FIFO order. |
| `MockMemory` | In-memory mock with episodic events and semantic key/value storage. |

### `apollia.testing.assertions`

| Function | Description |
| --- | --- |
| `assert_result_completed(result, contains=None)` | Assert status is `completed`. Optionally check text content. |
| `assert_result_failed(result, code=None)` | Assert status is `failed`. Optionally check error code. |
| `assert_result_input_required(result)` | Assert status is `input_required`. |
| `assert_tool_called(ctx, tool_name, times=None)` | Assert a tool was called (optionally N times). |
| `assert_llm_called(ctx, times=None)` | Assert the LLM was called (optionally N times). |

### `apollia.cli`

| Function | Description |
| --- | --- |
| `scaffold_agent(name, agent_type, output_dir)` | Generate agent + test files from a template. |
| `main(argv)` | CLI entry point for `apollia new`. |

## Testing Guide

The SDK ships with mock objects and assertion helpers so you can unit-test agents without a running Apollia runtime.

### 1. Create a MockContext

```python
from apollia.testing import MockContext

ctx = MockContext.create(
    tools={"bash_executor": {"output": "file1.py\nfile2.py"}},
    llm_responses=[{"content": '{"thought": "done", "action": "final_answer", "text": "Found 2 files"}'}],
    memory=True,
)
```

- `tools`: dict mapping tool names to their mock response dicts. Pass `None` to simulate no tools.
- `llm_responses`: list of response dicts consumed in order. Each `complete()` call pops the next.
- `memory`: `True` attaches an empty `MockMemory`.

### 2. Run the agent

```python
import pytest

@pytest.mark.asyncio
async def test_my_agent():
    ctx = MockContext.create(
        tools={"bash_executor": {"output": "hello"}},
        llm_responses=[{"content": "result"}],
    )
    agent = MyAgent()
    result = await agent.run({"input": {"parts": [{"type": "text", "text": "hi"}]}}, ctx)
```

### 3. Use assertion helpers

```python
from apollia.testing.assertions import (
    assert_result_completed,
    assert_result_failed,
    assert_tool_called,
    assert_llm_called,
)

# Verify the result
assert_result_completed(result)
assert_result_completed(result, contains="hello")

# Verify interactions
assert_tool_called(ctx, "bash_executor", times=1)
assert_llm_called(ctx, times=1)
```

### 4. Inspect mock internals

```python
# Tool calls are recorded
for name, args in ctx.tools.calls:
    print(f"Called {name} with {args}")

# LLM prompts are recorded
for prompt in ctx.llm.prompts:
    print(prompt)

# Memory operations are recorded
for op in ctx.memory.operations:
    print(op["op"], op)
```

### Complete test example

```python
"""Tests for my-agent."""

import pytest
from apollia.testing import MockContext
from apollia.testing.assertions import assert_result_completed, assert_tool_called

@pytest.mark.asyncio
async def test_agent_analyzes_project():
    ctx = MockContext.create(
        tools={"bash_executor": {"output": "src/\ntests/\nREADME.md"}},
        llm_responses=[
            {"content": '{"thought": "listing done", "action": "final_answer", "text": "3 items found"}'},
        ],
        memory=True,
    )
    agent = MyAgent()
    result = await agent.run(
        {"input": {"parts": [{"type": "text", "text": "list files"}]}},
        ctx,
    )
    assert_result_completed(result)
    assert_tool_called(ctx, "bash_executor", times=1)
```

## CLI Scaffolding

Generate a ready-to-run agent with one command:

```bash
# ReAct agent (default)
apollia new my-agent

# Conversational agent
apollia new my-chatbot --type conversational

# Orchestrated agent
apollia new my-planner --type orchestrated

# Custom output directory
apollia new my-agent --output-dir agents/
```

Each command generates two files:

| File | Content |
| --- | --- |
| `<name>_agent.py` | Agent class with `manifest()` and `run()` |
| `test_<name>_agent.py` | pytest test with `MockContext` and `assert_result_completed` |

You can also invoke scaffolding via Python:

```python
from apollia.cli import scaffold_agent

agent_path, test_path = scaffold_agent("my-agent", agent_type="react", output_dir=".")
```

## Migration from `apollia_base.py`

If you have agents using the ad-hoc approach (standalone `manifest()` and `run()` functions), here is how to migrate to the SDK.

### Before (without SDK)

```python
def manifest():
    return {"name": "my-agent", "version": "0.1.0", "tools": []}

async def run(task, ctx):
    result = await ctx.tools.call("bash_executor", {"command": "ls"})
    return {
        "status": "completed",
        "output": [{"type": "text", "text": str(result)}],
    }
```

### After (with SDK)

```python
from apollia.agents import BaseReActAgent, AIPResult
from apollia.utils.hitl import resume_pending_tool


class MyAgent(BaseReActAgent):
    SYSTEM_PROMPT = "You are a helpful assistant."

    def manifest(self):
        return {
            "name": "my-agent",
            "version": "0.1.0",
            "tools_required": ["bash_executor"],
            "execution_mode": "direct",
            "dangerous_tools_allowed": False,
        }

    async def run(self, task, ctx):
        user_msg = task["input"]["parts"][0]["text"]
        pending = resume_pending_tool(task)
        result = await self.react(task, ctx, user_msg, pending_tool=pending)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


agent = MyAgent()
```

### What changes

| Aspect | Before | After |
| --- | --- | --- |
| Structure | Loose functions | Class inheriting `BaseReActAgent` |
| ReAct loop | Manual implementation | Built-in via `self.react()` |
| HITL | Manual suspend/resume | `resume_pending_tool()` + `react(pending_tool=)` |
| Result format | Hand-built dicts | `AIPResult.completed()` / `.failed()` / `.input_required()` |
| Testing | Run against the full runtime | `MockContext` + assertion helpers |
| Type checking | None | `mypy` via `apollia.stubs` |

### Testing: before vs. after

**Before** — you had to start the runtime to test:

```bash
apollia-os start
apollia-os agent start my-agent
apollia-os run my-agent --input "test"
```

**After** — unit tests run instantly:

```python
import pytest
from apollia.testing import MockContext
from apollia.testing.assertions import assert_result_completed

@pytest.mark.asyncio
async def test_my_agent():
    ctx = MockContext.create(
        tools={"bash_executor": {"output": "file1.py"}},
        llm_responses=[{"content": '{"action": "final_answer", "text": "Done"}'}],
    )
    agent = MyAgent()
    result = await agent.run(
        {"input": {"parts": [{"type": "text", "text": "list files"}]}},
        ctx,
    )
    assert_result_completed(result, contains="Done")
```

### Compatibility

The old ad-hoc approach (module-level `manifest()` + `run()`) still works — the runtime's AIP loader checks for both class-based and function-based agents. No existing agent will break.

## Advanced: sdk-demo-agent

The repository includes a full-featured demo agent at `agents/sdk-demo-agent.py` that showcases every SDK feature:

- `BaseReActAgent` inheritance
- Tool introspection via `ctx.tools.describe()`
- Memory persistence (`ctx.memory.record()`, `recall()`, `remember()`)
- HITL suspension for destructive actions
- JSON extraction from LLM responses
- Markdown formatting for output

See `tests/test_sdk_demo_agent.py` for how to test it with `MockContext`.

## Project Structure

```
sdk/
├── pyproject.toml              # Package metadata, pip install config
├── README.md                   # This file
└── apollia/
    ├── __init__.py             # Package root, exports AIPResult + __version__
    ├── __main__.py             # python -m apollia entry point
    ├── types.py                # AIPResult dataclass
    ├── agents/
    │   ├── __init__.py         # Re-exports all base classes
    │   ├── react.py            # BaseReActAgent + AIPResult factory
    │   ├── conversational.py   # ConversationalAgent
    │   └── orchestrated.py     # OrchestratedAgent
    ├── stubs/
    │   ├── __init__.py         # Re-exports all type stubs
    │   ├── context.py          # RuntimeContext stub
    │   ├── tools.py            # ToolProxy stub
    │   ├── llm.py              # LlmProxy, LlmResponse, TokenUsage stubs
    │   └── memory.py           # MemoryInterface stub
    ├── tools/
    │   ├── __init__.py         # Re-exports schemas
    │   └── schemas.py          # Native tool schemas for LLM prompts
    ├── utils/
    │   ├── __init__.py         # Re-exports all utilities
    │   ├── parsing.py          # JSON/code/XML extraction, truncation
    │   ├── formatting.py       # Text, Markdown, JSON formatting
    │   └── hitl.py             # HITL resume helper
    ├── testing/
    │   ├── __init__.py         # Re-exports mocks + assertions
    │   ├── mocks.py            # MockContext, MockToolProxy, MockLlmProxy, MockMemory
    │   └── assertions.py       # assert_result_completed, assert_tool_called, etc.
    └── cli/
        ├── __init__.py         # Re-exports scaffold_agent + main
        ├── __main__.py         # Console-scripts entry point
        └── scaffold.py         # Agent scaffolding templates + CLI
```

## License

Apache-2.0 — see the root repository for details.
