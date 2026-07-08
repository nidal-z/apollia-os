---
sidebar_position: 1
title: Drive Apollia from your product
---

# Drive Apollia from your product

In this tutorial you start a real Apollia daemon, authenticate to it, and drive
it from a host program through the generated Python client. By the end you will
have submitted a task to an agent and read its result back, entirely over the
local HTTP API.

This is the integration path: your product talks to an Apollia runtime the same
way any host application does, without embedding any Rust or reverse
engineering the wire format.

## What you will build

A small Python script that:

1. reads the daemon's API token,
2. opens an authenticated client,
3. submits a task to an agent,
4. polls until the task completes and prints the result.

You will use a no-LLM `echo` agent so the tutorial runs on any machine, with no
model download required.

## Before you start

You need a checkout of the Apollia repository, a Rust toolchain to build the
daemon, and Python 3.12 or newer. Every command below is run from the
repository root.

## Step 1: build the daemon

```sh
cargo build -p apollia-cli
```

This produces the `apollia-os` binary at `target/debug/apollia-os`. For brevity
the rest of the tutorial calls it `apollia-os`; use the full path if it is not
on your `PATH`.

## Step 2: install the echo agent

The repository ships a minimal agent that echoes its input. Install it so the
daemon can load it:

```sh
apollia-os agent install clients/examples/echo_agent.py --skip-tests
```

## Step 3: start the daemon

Start the runtime. It listens on a Unix socket and, with a TCP port, on
`127.0.0.1`:

```sh
apollia-os start --port 7771
```

On first boot the daemon generates an API token and writes it to
`~/.apollia/api-token` (readable only by you). TCP callers must present this
token as a bearer credential. The Unix socket is local-trust and needs none.

Leave this running and open a second terminal for the next steps.

## Step 4: set up the Python client

The generated client lives at `clients/python`. Its runtime dependencies are
`httpx`, `attrs`, and `python-dateutil`:

```sh
python3 -m venv clients/.venv
clients/.venv/bin/pip install httpx attrs python-dateutil
```

## Step 5: drive the daemon

Save this as `drive.py`. It reads the token, opens an authenticated client,
submits a task to the `echo` agent, and polls for the result:

```python
import pathlib
import sys
import time

# Make the generated client importable.
sys.path.insert(0, "clients/python")

from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.health import health_handler
from apollia_runtime_client.api.tasks import get_task, submit_task
from apollia_runtime_client.models import SubmitTaskRequest, SubmitTaskRequestInput

TOKEN = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()
TERMINAL = {"completed", "succeeded", "done", "failed", "error", "cancelled"}

client = AuthenticatedClient(base_url="http://127.0.0.1:7771", token=TOKEN)

health = health_handler.sync(client=client)
print("health:", health.status)

submitted = submit_task.sync(
    client=client,
    body=SubmitTaskRequest(
        agent_id="echo",
        input_=SubmitTaskRequestInput.from_dict(
            {"parts": [{"type": "text", "text": "hello from the host SDK"}]}
        ),
    ),
)
print("submitted:", submitted.task_id, submitted.status)

task = submitted
for _ in range(120):
    task = get_task.sync(id=submitted.task_id, client=client)
    if task.status.lower() in TERMINAL or task.result:
        break
    time.sleep(0.5)

print("status:", task.status)
print("result:", task.result)
```

Run it with the virtual environment's interpreter:

```sh
clients/.venv/bin/python drive.py
```

You will see the health check succeed, the task submitted with an identifier,
and the echoed result read back. That is a host application driving a real
Apollia runtime end to end.

## What just happened

Every call went through the generated client, which is produced from the
runtime's OpenAPI specification. Nothing was hand-written against the wire
format, so the client cannot drift from the contract. You authenticated over TCP
with a bearer token, submitted work to an agent, and retrieved its result, which
is the whole shape of a product integration.

## Clean up

Stop the daemon in the first terminal, or run:

```sh
apollia-os stop
```

## Where to go next

- To integrate this into a real product, read
  [Integrate Apollia via the driving contract](/how-to/integrate-via-driving-contract).
- For every endpoint, request, and response, see the
  [HTTP API reference](/reference/api/apollia-os-runtime-api).
- A runnable version of this flow lives at `clients/examples/demo_python.sh`,
  which builds, starts, drives, and tears down the daemon in one command.
