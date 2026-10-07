---
sidebar_position: 2
title: Your first agent
description: "Build, install and run your first Apollia package: a small product coach declared in apollia.toml, with one Python tool, from an empty folder to a working answer."
---

# Your first agent

In this tutorial you build a small package: a product coach that answers
questions about Apollia from a glossary. The agent is a few lines of
`apollia.toml`; the glossary is one Python tool of about thirty lines. You
install the package into a running Apollia runtime and talk to the agent from
the command line. Plan for around fifteen minutes.

By the end you will have seen the whole loop that every package goes through:
declare the agent, write a tool, describe, build, install, run.

## Before you start

- Apollia installed, with the `apollia-os` command on your `PATH`.
- The SDK importable by the Python you work with (`pip install -e ./sdk` from a
  clone of the repository, in a virtual environment). It is needed to describe
  and to test the package, not to build or install it.
- **The runtime running**, in a second terminal:

  ```sh
  apollia-os start --port 7771
  ```

  `package install`, `run` and `llm status` all talk to it; without it they exit
  with `runtime not started (connection refused)`. Leave it running for the whole
  tutorial and stop it with `apollia-os stop` at the end.
- An LLM backend configured. Check it with `apollia-os llm status`. If nothing
  is set up yet, register a local model with
  `apollia-os llm setup --local --model /path/to/model.gguf`, or a cloud backend
  with `apollia-os llm backends create --provider <p> --model <m> --api-key <key>`.
  See
  [Install and run the runtime](/how-to/install-and-run#step-3-configure-a-model-backend)
  and the [CLI reference](/reference/cli) for every `llm` subcommand.

You do not need to know the SDK yet. This tutorial introduces one decorator;
the rest is ordinary Python and a TOML file.

## Step 1: declare the agent

A package is a folder with an `apollia.toml` at its root. Create a folder
called `coach`, and in it the file `apollia.toml`:

```toml
[package]
name = "coach"
version = "0.1.0"
publisher = "You"
description = "A product coach that answers questions about Apollia from a small glossary."
sdk = ">=0.2.0-preview"

[[agents]]
id = "coach"
name = "Coach"
mission = """
You are a product coach for Apollia. Answer in the language of the person, in
two to four sentences.

When the person asks what a term means, call the tool `define` with that term
and build your answer on the definition it returns. When the tool knows no
such term, say so plainly, list the terms it knows, and point the person to
the documentation. Never invent a definition.
"""
examples = [
    "What is a package?",
    "What is the difference between a tool and a workflow?",
]

[agents.defaults]
access = ["pkg:coach/define"]
autonomy = "assisted"
```

That is the whole agent:

- **`[[agents]]`** declares it. `mission` is its instructions, written as you
  would brief a colleague. `examples` are the requests its card shows.
- **`[agents.defaults]`** says what the package recommends. `access` lists the
  tools the agent may call, one by one: `pkg:coach/define` is the tool `define`
  of the package `coach`, which the next step writes. The person who installs
  the package can grant or revoke each access afterwards; a default is never a
  constraint.

<!-- claim:module-level-agent-attribute-is-the-entry-point -->

An agent is data. No Python class or object stands for it, and the runtime
loads no agent from a Python module: the engine built into the runtime runs
the agent from this entry, with its mission, the tools it was granted and the
model you configured.

## Step 2: write the tool

The Python of a package brings tools. A tool is an `async` function under
`@tool`. Create `coach/tools/glossary.py`:

```python
"""The glossary tool of the package coach."""

from typing import Annotated

from apollia import tool
from apollia.types import Ctx

GLOSSARY = {
    "agent": "A mission, examples and granted tools, written as data in apollia.toml.",
    "package": "A folder with an apollia.toml: any mix of agents, tools and workflows.",
    "tool": "An async Python function under @tool, which an agent may be granted.",
    "workflow": "An async Python function under @workflow, whose steps are declared ahead.",
}


@tool(
    "define",
    "Look a term up in the Apollia glossary.",
    action="read",
    examples=[{"term": "package"}],
)
async def define(
    ctx: Ctx,
    term: Annotated[str, "The term to define, one or two words."],
) -> dict[str, object]:
    key = term.strip().lower()
    ctx.logger.info("define called for %s", key)
    if key not in GLOSSARY:
        return {"found": False, "known_terms": sorted(GLOSSARY)}
    return {"found": True, "term": key, "definition": GLOSSARY[key]}
```

Four things to read here:

- **The name and the description** are what the model sees. The name is the
  last part of `pkg:coach/define`.
- **`action="read"`** is the family of the tool: what calling it does to the
  world. There are five (`read`, `read_personal`, `write_or_delete`,
  `send_external`, `execute_code`), and every family except `read` is
  sensitive: the runtime asks the person before such a tool runs.
- **The signature is the schema.** `ctx` comes first and is left out; every
  other parameter is an input, and `Annotated` carries the description the
  model reads. `examples` are sample inputs shown beside the schema.
- **`ctx`** is the context the runtime hands the function. This tool only uses
  `ctx.logger`, a standard `logging.Logger`. The full contract is the
  [SDK / ctx reference](/reference/sdk).

Use absolute imports (`from apollia import ...`), never relative ones.

## Step 3: test the tool

A tool is a plain `async def` function: a test awaits it as written, with a
`MockContext` in place of the runtime. Create `coach/tests/test_glossary.py`:

```python
import asyncio

from apollia.testing import MockContext

from tools.glossary import define


def test_define_returns_the_definition_of_a_known_term():
    # GIVEN a context and a term of the glossary, in another case
    ctx = MockContext()

    # WHEN the tool is awaited as written
    result = asyncio.run(define(ctx, " Package "))

    # THEN the definition comes back under the normalised term
    assert result["found"] is True
    assert result["term"] == "package"
```

Run it from the package folder:

```sh
cd coach
python -m pytest -p no:cacheprovider tests
cd ..
```

`-p no:cacheprovider` keeps pytest from leaving a `.pytest_cache` folder
behind: the build of the next step takes every file of the package folder.

## Step 4: describe and build

`apollia.toml` must say which tools the Python defines, so that reading the
manifest never runs the code. You do not write those entries by hand:

```sh
python -m apollia describe coach
```

The command imports the modules of the folder and writes a `[[tools]]` entry
for `define` at the end of `apollia.toml`: its module, its description, its
family and its input schema. Run it again after every change to a decorated
function.

Then build the archive and read it back:

```sh
apollia-os package build coach
apollia-os package inspect coach-0.1.0.apo
```

`build` validates the manifest and writes `coach-0.1.0.apo` in the current
folder. No Python runs. If the manifest names a tool the package does not
define, or the Python defines one the manifest does not describe, this is
where you find out. `inspect` prints the manifest, the entries of the archive
and its signature state, `unsigned` here.

## Step 5: install

`preview` shows what the package contains and asks for, before anything is
written. `install` copies it into the package store of the runtime:

```sh
apollia-os package preview coach-0.1.0.apo
apollia-os package install coach-0.1.0.apo
```

An unsigned package installs with a notice. Confirm the agent is there:

```sh
apollia-os agent list --definitions
```

## Step 6: talk to it

`run` sends one message to the agent and prints the reply:

```sh
apollia-os run coach "What is a package?"
```

You should get a two to four sentence answer built on the glossary. Ask it
about a term the glossary does not hold and it tells you so, because the
mission instructed it to.

`apollia-os run` is a single call. For an ongoing back and forth, use
`apollia-os chat` or the desktop app, where the agent appears on the Agents
screen with its mission, its examples and its accesses.

## What you built

A package with one agent and one tool. The agent is data in `apollia.toml`: a
mission, examples, and the list of what it may reach. The tool is a Python
function the agent was granted. That is the smallest complete package that
carries code; a package with only `[[agents]]` and no Python is valid too.

## Next steps

- Change the mission, build and install again: an update keeps the changes the
  person made to the agent on their side.
- Start the next package from the folder `apollia-os package new <name>`
  writes, which builds as written.
- Sign the archive with `apollia-os package build coach --sign`, so that the
  person who installs it sees who published it.
- Chain several agents and a question to the user in a workflow:
  [Build a multi-agent assistant](/tutorials/build-a-multi-agent-assistant).
