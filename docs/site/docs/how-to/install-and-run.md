---
sidebar_position: 7
title: Install and run the runtime
---

# Install and run the runtime

Apollia is source-available: you build it from a checkout, there is no package on
crates.io or PyPI to install. This guide takes you from a clone to a running
daemon that can execute an agent, on Linux or macOS. Plan for one longer first
build (Rust compiles the workspace once), then rebuilds are incremental.

Every command runs from the repository root.

## Prerequisites

- A Rust toolchain (stable). Install it from [rustup.rs](https://rustup.rs) if you
  do not have `cargo`.
- Python 3.12 or newer, available as `python3`. The runtime embeds Python to load
  agents, and the SDK installs into it.
- `git`. It is required to clone the repository and to install agents from a Git
  URL later.

On macOS, PyO3 must find the right interpreter at build time. If the default
`python3` is not the one you want, point it explicitly before building:

```sh
export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13
```

## Step 1: clone and build the daemon

```sh
git clone https://github.com/Apollia-OS/apollia-os.git
cd apollia-os
cargo build -p apollia-cli
```

The crate is named `apollia-cli` but the binary it produces is `apollia-os`, at
`target/debug/apollia-os`. That naming is deliberate; do not look for a file
called `apollia-cli`. Put the binary on your `PATH` so the rest of this guide can
call it by name:

```sh
export PATH="$PWD/target/debug:$PATH"
```

This default build is cloud-capable: it talks to Anthropic, OpenAI-compatible, or
Vertex backends. Local GGUF inference needs one extra component, covered in the
last section.

## Step 2: install the SDK

Agents are Python. Install the `apollia` package in editable mode into the same
interpreter the runtime uses:

```sh
pip install -e ./sdk
```

## Step 3: configure a model backend

An agent that generates text needs a backend. Pick one path.

Cloud: authenticate to a provider and register a backend.

```sh
apollia-os auth login anthropic
apollia-os llm backends create prod --provider anthropic --model claude-sonnet-4-6 --default
```

Local: point the runtime at a `.gguf` file on your machine. This registers the
backend but does not itself run inference (see the last section for the local
inference engine).

```sh
apollia-os llm setup --local --model /path/to/model.gguf
```

Either way, confirm the backend is visible:

```sh
apollia-os llm status
```

## Step 4: start the daemon

```sh
apollia-os start --port 7771
```

The runtime listens on a Unix socket (`/tmp/apollia.sock` by default) and, with a
port, on `127.0.0.1:7771`. On first boot it writes an API token to
`~/.apollia/api-token`; TCP callers must present it as a bearer credential, while
the Unix socket is local-trust and needs none. Leave this terminal running and
open a second one for the next steps.

## Step 5: run an agent

The repository ships a no-LLM `echo` agent that runs on any machine. Install it,
then send it a task:

```sh
apollia-os agent install clients/examples/echo_agent.py --skip-tests
apollia-os run echo "hello from Apollia"
```

You should see the echoed result. To write your own agent from scratch, follow
[Your first agent](/tutorials/your-first-agent).

## Step 6: stop the daemon

```sh
apollia-os stop
```

## Optional: enable local GGUF inference

The default `apollia-os` binary is cloud-only. Local inference is served by a
separate sidecar, `apollia-runner`, which links llama.cpp with a hardware
backend. The daemon spawns it on demand and looks for a runner binary named
`apollia-runner-<backend>` sitting in the same directory as `apollia-os`.

Build the runner for your hardware (choose exactly one backend) and co-locate it:

```sh
# Apple Silicon
cargo build -p apollia-runner --release --features local-metal
cp target/release/apollia-runner target/release/apollia-runner-metal

# Portable CPU
cargo build -p apollia-runner --release --features local-cpu
cp target/release/apollia-runner target/release/apollia-runner-cpu

# NVIDIA (needs the CUDA toolkit)
cargo build -p apollia-runner --release --features local-cuda
cp target/release/apollia-runner target/release/apollia-runner-cuda
```

The other backends are `local-rocm` (AMD) and `local-vulkan` (cross-vendor). Put
the suffixed runner next to the `apollia-os` binary you run. If a local backend is
configured but no matching runner is found, LLM calls fail with a
`503 Service Unavailable` and a `BackendUnavailable` reason; build and co-locate
the runner to resolve it.

To download and manage `.gguf` files, see the `model` commands in the
[CLI reference](/reference/cli).

## Next steps

- Write and run your own agent: [Your first agent](/tutorials/your-first-agent).
- Run Apollia as a managed service: [Deploy in production](/how-to/deploy-in-production).
- Every flag on every command is in the [CLI reference](/reference/cli).
