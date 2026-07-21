---
sidebar_position: 7
title: Install and run the runtime
---

# Install and run the runtime

Apollia is source-available: you build it from a checkout, there is no package on
crates.io or PyPI to install. This guide is for developers. It takes you from a
clone to a running daemon that can execute an agent, and then to the Tauri desktop
app in dev mode, on macOS, Linux, or Windows. Plan for one longer first build
(Rust compiles the workspace once), then rebuilds are incremental.

If you only want to run the finished desktop application, download a prebuilt
installer instead: [Install the desktop app](/how-to/install-the-desktop-app).

Every command runs from the repository root unless stated otherwise.

## Prerequisites

The runtime needs a Rust toolchain, Python, and Git on every platform. The
desktop app adds Node.js, the `cargo tauri` CLI, and a per-OS webview toolchain.
Install the shared tools first, then the OS-specific section for your machine.

### Shared (all platforms)

- **Rust toolchain (stable).** Install it from [rustup.rs](https://rustup.rs) if
  you do not have `cargo`. The repository pins an exact compiler in
  `rust-toolchain.toml`; `rustup` reads that pin and installs the matching
  toolchain automatically the first time you build inside the checkout, so you do
  not select a version by hand.
- **Python 3.13**, available as `python3`. The runtime embeds Python to load
  agents, and the SDK installs into it. The checkout declares the exact version in
  `.python-version`.
- **Git.** Required to clone the repository and to install agents from a Git URL
  later.
- **For the desktop app only: Node.js 20 or newer** (the project builds the UI on
  Node 22) with `npm`, and the `cargo tauri` CLI:

  ```sh
  cargo install tauri-cli --version "^2"
  ```

- **For a local inference runner only** (the optional last section): CMake and a
  C/C++ compiler. The runner compiles llama.cpp from source, which needs both. The
  default cloud-capable build does not.

### macOS

- **Xcode Command Line Tools**, for the C/C++ toolchain and the WebKit webview the
  desktop app renders into:

  ```sh
  xcode-select --install
  ```

- **PyO3 must find the right interpreter at build time.** If the default `python3`
  is not the one you want, point it explicitly before building. With Homebrew
  Python:

  ```sh
  export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13
  ```

- The optional local runner needs CMake (`brew install cmake`); the compiler comes
  from the Command Line Tools above.
- The desktop app requires macOS 13 (Ventura) or newer.

### Linux (Debian / Ubuntu)

Install the build toolchain plus the Tauri v2 webview and system libraries. On
Debian and Ubuntu:

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config libssl-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev \
  libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
  libasound2-dev libpulse-dev libjack-jackd2-dev \
  python3-dev clang cmake file
```

What each group is for:

- `build-essential pkg-config libssl-dev`: C/C++ toolchain and linking.
- `libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
  libsoup-3.0-dev libjavascriptcoregtk-4.1-dev`: the Tauri v2 webview and tray
  dependencies. Needed only for the desktop app.
- `libasound2-dev libpulse-dev libjack-jackd2-dev`: audio headers for the desktop
  app's speech-to-text capture. Needed only for the desktop app.
- `python3-dev`: headers for the embedded Python.
- `clang cmake`: needed only if you build a local inference runner.

If you are building only the command-line runtime and not the desktop app, you can
skip the webview and audio groups and install just `build-essential pkg-config
libssl-dev python3-dev` (plus `clang cmake` for a local runner).

On other distributions install the equivalents of the same libraries. Package
names differ (for example WebKitGTK 4.1, GTK 3, libayatana-appindicator, and
librsvg development packages); verify the exact names for your distribution
against the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/).

### Windows

- **Microsoft C++ Build Tools** (the "Desktop development with C++" workload from
  the Visual Studio Build Tools installer), for the MSVC compiler and linker.
- **Microsoft Edge WebView2 runtime.** It is preinstalled on current Windows 11
  and updated Windows 10. If it is missing, install the "Evergreen Bootstrapper"
  from Microsoft's WebView2 Runtime download page. Needed only for the desktop
  app.
- **CMake**, only if you build a local inference runner.
- Run the commands below from a shell where `cargo`, `python`, `git`, and (for the
  desktop app) `npm` are on `PATH`. The runtime primitives are tested on macOS and
  Linux; on Windows, verify the daemon commands on your machine and prefer a
  developer PowerShell that has the MSVC environment loaded.

## Step 1: clone and build the daemon

```sh
git clone https://github.com/Apollia-OS/apollia-os.git
cd apollia-os
cargo build -p apollia-cli
```

Build the `apollia-cli` crate specifically. A workspace-wide build
(`cargo build --workspace`) also pulls in the desktop crate, which requires the
full webview toolchain from the platform sections above; scope the build to
`apollia-cli` to compile just the runtime.

The crate is named `apollia-cli` but the binary it produces is `apollia-os`, at
`target/debug/apollia-os`. That naming is deliberate; do not look for a file
called `apollia-cli`. Put the binary on your `PATH` so the rest of this guide can
call it by name:

```sh
export PATH="$PWD/target/debug:$PATH"
```

On Windows the binary is `target\debug\apollia-os.exe`; add `target\debug` to your
`PATH` the equivalent way.

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
apollia-os llm backends create prod --provider anthropic --model claude-sonnet-4-20250514 --default
```

Use your provider's current model id for `--model`; the value above is only an
example.

Local: point the runtime at a `.gguf` file on your machine. This registers the
backend but does not itself run inference (see the last section for the local
inference engine).

```sh
apollia-os llm setup --local --model /path/to/model.gguf
```

Either way, reload the backend registry and confirm the backend is visible. A
freshly configured backend does not appear in `llm status` until you reload:

```sh
apollia-os llm reload
apollia-os llm status
```

## Step 4: start the daemon

```sh
apollia-os start --port 7771
```

On its first run the runtime creates its data directory at `~/.apollia`. It
listens on a Unix socket (`/tmp/apollia.sock` by default) and, with a port, on
`127.0.0.1:7771`. On first boot it writes an API token to `~/.apollia/api-token`;
TCP callers must present it as a bearer credential, while the Unix socket is
local-trust and needs none. Leave this terminal running and open a second one for
the next steps.

## Step 5: run an agent

The repository ships a no-LLM `echo` agent that runs on any machine. Install it,
enable it so the runtime loads it, then send it a task:

```sh
apollia-os agent install clients/examples/echo_agent.py --skip-tests
apollia-os agent enable echo
apollia-os run echo "hello from Apollia"
```

Without the `enable` step, `run` reports `agent 'echo' not found`.

You should see the echoed result. To write your own agent from scratch, follow
[Your first agent](/tutorials/your-first-agent).

## Step 6: stop the daemon

```sh
apollia-os stop
```

## Run the desktop app in dev mode

The Tauri desktop app is the graphical front end over the same runtime. To bring
it up from source, install the frontend dependencies once, then start the dev
build. This needs Node.js, the `cargo tauri` CLI, and the per-OS webview
prerequisites from the sections above.

Install the Svelte UI dependencies (a `just` recipe wraps `npm ci`):

```sh
just desktop-ui-install
# equivalent to: cd crates/apollia-desktop/ui && npm ci
```

Start the app in dev mode. This launches the Vite dev server for the UI and the
Tauri shell with hot reload:

```sh
just desktop-dev
# equivalent to: cd crates/apollia-desktop && cargo tauri dev
```

The first `cargo tauri dev` compiles the desktop crate and can take a while;
subsequent runs are incremental. The window uses the system webview (WebKit on
macOS and Linux, WebView2 on Windows), so make sure your platform's webview
prerequisites are installed.

For local inference inside the dev app, build a runner (next section) and place it
next to the desktop's `apollia-os` binary, or drive the desktop against an
external `llama-server`; see [Accelerate local inference](/how-to/accelerate-local-inference).

## Optional: enable local GGUF inference

The default `apollia-os` binary is cloud-only. Local inference is served by a
separate sidecar, `apollia-runner`, which links llama.cpp with a hardware
backend. The daemon spawns it on demand and looks for a runner binary named
`apollia-runner-<backend>` sitting in the same directory as `apollia-os`.

Building the runner compiles llama.cpp from source, so CMake and a C/C++ compiler
must be installed (see the prerequisites for your platform).

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

There is no download command. Obtain a `.gguf` file yourself (for example from a
model hub) and place it in `~/.apollia/models/`, then point a local backend at it
with `apollia-os llm setup --local --model <path.gguf>`. The `model` subcommands
(`list`, `search`, `show`, `hardware`, `delete`) inspect and manage the models
already present; see the [CLI reference](/reference/cli).

## Next steps

- Write and run your own agent: [Your first agent](/tutorials/your-first-agent).
- Run Apollia as a managed service: [Deploy in production](/how-to/deploy-in-production).
- Every flag on every command is in the [CLI reference](/reference/cli).
