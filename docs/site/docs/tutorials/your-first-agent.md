---
sidebar_position: 2
title: Your first agent
---

# Your first agent

In this tutorial you build a small conversational agent, a product coach that
answers questions about Apollia from an embedded knowledge base. You will write
about seventy lines of Python, install the agent into a running Apollia daemon,
and talk to it from the command line. Plan for around fifteen minutes.

By the end you will have seen the whole loop that every agent goes through:
write a class, declare an entry point, install, enable, run.

## Before you start

- Apollia installed, with the `apollia-os` command on your `PATH`.
- **The daemon running**, in a second terminal:

  ```sh
  apollia-os start --port 7771
  ```

  `agent enable`, `run` and `llm status` all talk to it; without it they exit
  with `runtime not started (connection refused)`. Leave it running for the whole
  tutorial and stop it with `apollia-os stop` at the end.
- An LLM backend configured. Check it with `apollia-os llm status`. If nothing
  is set up yet, register a local model with
  `apollia-os llm setup --local --model /path/to/model.gguf`, or a cloud backend
  with `apollia-os llm backends create --provider <p> --model <m> --api-key <key>`.
  See
  [Install and run the runtime](/how-to/install-and-run#step-3-configure-a-model-backend)
  and the [CLI reference](/reference/cli) for every `llm` subcommand.

You do not need to know the SDK yet. This tutorial introduces one decorator and
one service; the rest is ordinary Python.

## Step 1: write the agent

Create a file called `coach.py`:

```python
"""A friendly conversational product coach for Apollia OS."""

from apollia import agent, on_message
from apollia.types import Ctx, Message


KNOWLEDGE_BASE = """
Apollia OS is a local runtime for autonomous AI agents.

Three agent patterns:
- Worker: exposes A2A skills (a pdf worker, a chart worker, and so on).
- Conversational: replies to a human through @on_message.
- Director: orchestrates workers through apollia.react.

Three essential Ctx services:
- ctx.llm: generation.
- ctx.memory: episodic and semantic persistence.
- ctx.a2a: calling other agents.
"""

SYSTEM_PROMPT = f"""\
You are a helpful product coach for Apollia OS. Answer in the user's
language. Stay concise, two to four sentences. When the user asks for a
capability that is not in the knowledge base below, say so honestly and
point them to the documentation.

KNOWLEDGE BASE
==============
{KNOWLEDGE_BASE}
"""


@agent(
    name="coach",
    version="0.1.0",
    description="Friendly product coach for Apollia OS users.",
    agent_type="assistant",
)
class Coach:
    @on_message
    async def chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> str:
        response = await ctx.llm.complete(
            messages=[
                {"role": "system", "content": SYSTEM_PROMPT},
                *history,
                {"role": "user", "content": message},
            ],
        )
        return response.content
```

Three things make this an agent:

- **`@agent(...)`** declares the manifest. `name`, `version`, and `description`
  are required. `agent_type` is an optional label.
- **`@on_message`** marks the single conversational entry point. Its signature is
  fixed: `(self, message, history, ctx)` returning the reply as a string.
<!-- claim:module-level-agent-attribute-is-the-entry-point -->

- **A module-level `agent` symbol** is what the runtime loads. You do not write
  it: `@agent` instantiates the class and binds the instance to the module for
  you. Use absolute imports (`from apollia import ...`), never relative ones.

Inside the handler, `ctx.llm.complete(...)` sends the conversation to the
configured backend and returns a response whose `.content` is the generated
text. The exact shape of every `ctx` service lives in the
[SDK / ctx contract](/reference/sdk); this tutorial only needs
[`ctx.llm`](/reference/sdk/llm).

## Step 2: inspect it

Before installing anything, check the file statically. `inspect` reads the
manifest and reports what the agent declares, without starting a runtime:

```bash
apollia-os inspect coach.py
```

If you mistyped a decorator argument or forgot the module-level `agent = ...`,
this is where you find out.

## Step 3: install and enable

Install copies the file into Apollia's agent store, then enable makes it
loadable:

```bash
apollia-os agent install ./coach.py
apollia-os agent enable coach
```

Confirm it is active:

```bash
apollia-os agent list
```

## Step 4: talk to it

`run` sends one message to the agent and prints the reply:

```bash
apollia-os run coach "How does the Director pattern work?"
```

You should get a two to four sentence answer drawn from the knowledge base. Ask
it something outside the knowledge base and it will tell you it does not know,
because the system prompt instructed it to.

`apollia-os run` is a single call. For an ongoing back and forth, use
`apollia-os chat` or the desktop app; both keep the `history` that your handler
already accepts.

## What you built

A conversational agent is a class with one `@on_message` method that turns an
incoming message into a reply, using `ctx.llm` for generation. That is the
smallest complete agent Apollia runs.

## Next steps

- Stream the answer token by token with
  [`ctx.llm`](/reference/sdk/llm) `stream` and
  [`ctx.events`](/reference/sdk/events) `emit_token`, so the user sees text as
  it is produced.
- Give the agent memory by adding `memory_namespace="coach"` to `@agent` and
  recording turns with [`ctx.memory`](/reference/sdk/memory). Memory is opt in
  per agent: without a namespace, `ctx.memory` is unavailable by design.
- Expose reusable capabilities instead of a chat loop:
  [Write a worker](/how-to/write-a-worker).
- Let an agent orchestrate several workers:
  [Write a director](/how-to/write-a-director).
