---
sidebar_position: 1
title: Drive Apollia from your product
description: Start a real Apollia daemon, authenticate to it, and run a workflow from your own code through the HTTP API, from first call to result.
---

# Drive Apollia from your product

In this tutorial you start a real Apollia daemon, authenticate to it, and drive
it from a host program through the generated Python client. By the end you will
have launched a workflow and read its result back, entirely over the local
HTTP API.

This is the integration path: your product talks to an Apollia runtime the same
way any host application does, without embedding any Rust or reverse
engineering the wire format.

## What you will build

A small Python script that:

1. reads the daemon's API token,
2. opens an authenticated client,
3. launches a workflow,
4. polls until the run ends and prints the result.

You will use the `echo` package, whose single workflow returns the message it
receives. A workflow runs its steps as written, so the tutorial needs no model
and runs on any machine.

## Before you start

You need a checkout of the Apollia repository, a Rust toolchain to build the
daemon, and Python 3.13, the version the checkout pins in `.python-version` and
the one every job that builds the runtime uses. Every command below is run from
the repository root.

## Step 1: build the daemon

```sh
cargo build -p apollia-cli
```

This produces the `apollia-os` binary at `target/debug/apollia-os`. For brevity
the rest of the tutorial calls it `apollia-os`; use the full path if it is not
on your `PATH`.

## Step 2: build the echo package

The repository ships a minimal package at `clients/examples/echo`: an
`apollia.toml` and one workflow, `echo`, declared with `@workflow` in
`workflows/echo.py`. It declares no agent. Build its archive:

```sh
apollia-os package build clients/examples/echo
```

This writes `echo-0.1.0.apo` in the current folder. No Python code runs: the
build reads the workflow from the descriptor that
`python -m apollia describe clients/examples/echo` wrote into `apollia.toml`.

## Step 3: start the daemon

<!-- claim:daemon-binds-tcp-by-default -->
Start the runtime. It listens on a Unix socket (a named pipe on Windows, which
has none) and on `127.0.0.1:7771`. The TCP port is always bound; `--port`
chooses the number:

```sh
apollia-os start --port 7771
```

On first boot the daemon generates an API token and writes it to
`~/.apollia/api-token` (readable only by you). TCP callers must present this
token as a bearer credential. The Unix socket is local-trust and needs none.

Leave this running and open a second terminal for the next steps.

## Step 4: install the package

In the second terminal, install the archive into the running daemon:

```sh
apollia-os package install echo-0.1.0.apo
```

An unsigned package installs with a notice. Confirm the workflow is there:

```sh
apollia-os workflow show echo/echo
```

It prints the single step and the `message` input.

## Step 5: set up the Python client

The generated client lives at `clients/python`. Its runtime dependencies are
`httpx`, `attrs`, and `python-dateutil`:

```sh
python3 -m venv clients/.venv
clients/.venv/bin/pip install httpx attrs python-dateutil
```

## Step 6: drive the daemon

Save this as `drive.py`. It reads the token, opens an authenticated client,
launches the `echo` workflow of the `echo` package, and polls the run to its
end:

```python
import pathlib
import sys
import time

# Make the generated client importable.
sys.path.insert(0, "clients/python")

from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.health import health_handler
from apollia_runtime_client.api.workflows import get_workflow_run, launch_workflow
from apollia_runtime_client.models import WorkflowLaunch, WorkflowLaunchInputs

TOKEN = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()

client = AuthenticatedClient(base_url="http://127.0.0.1:7771", token=TOKEN)

health = health_handler.sync(client=client)
print("health:", health.status)

launched = launch_workflow.sync(
    "echo",
    "echo",
    client=client,
    body=WorkflowLaunch(
        inputs=WorkflowLaunchInputs.from_dict({"message": "hello from the host SDK"})
    ),
)
print("launched:", launched.run_id, launched.status)

run = launched
for _ in range(120):
    run = get_workflow_run.sync(launched.run_id, client=client).run
    if run.status != "running":
        break
    time.sleep(0.5)

print("status:", run.status)
print("result:", run.output.to_dict())
```

Run it with the virtual environment's interpreter:

```sh
clients/.venv/bin/python drive.py
```

You will see the health check succeed, the run launched with an identifier,
and the result `{'echo': 'hello from the host SDK'}` read back. That is a host
application driving a real Apollia runtime end to end.

## What just happened

Every call went through the generated client, which is produced from the
runtime's OpenAPI specification. Nothing was hand-written against the wire
format, so the client cannot drift from the contract. You authenticated over TCP
with a bearer token, launched a workflow, and retrieved its result, which is the
whole shape of a product integration.

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
  which builds, installs, starts, drives, and tears down the daemon in one
  command. It refuses to run while another runtime is up, so stop yours first.
