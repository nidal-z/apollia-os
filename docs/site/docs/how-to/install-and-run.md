---
sidebar_position: 7
title: Install and run the runtime
description: "Build Apollia OS from source and run the runtime: prerequisites, the build, the first start, and how to check it is actually serving."
---

# Install and run the runtime

Apollia publishes no package on crates.io or PyPI yet, so you build it from a
checkout. This guide is for developers. It takes you from a
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

- **For local inference:** the daemon serves local GGUF models through an embedded
  `llama-server`. A packaged build bundles it; on a source build you need
  `llama-server` on your `PATH` (see the last section), no compiler required.
  Building the optional speech-to-text runner from source additionally needs CMake
  and a C/C++ compiler.

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

- Building the optional speech-to-text runner from source needs CMake
  (`brew install cmake`); the compiler comes from the Command Line Tools above.
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
- `clang cmake`: needed only if you build the speech-to-text runner from source.

If you are building only the command-line runtime and not the desktop app, you can
skip the webview and audio groups and install just `build-essential pkg-config
libssl-dev python3-dev` (plus `clang cmake` for the speech-to-text runner).

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
- **CMake**, only if you build the speech-to-text runner from source.
- **LLVM** (provides `libclang.dll` for bindgen), only if you build the
  speech-to-text runner from source. Install with
  `winget install LLVM.LLVM`, then point bindgen at it before building:

  ```powershell
  $env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
  $env:CMAKE_MSVC_RUNTIME_LIBRARY = "MultiThreaded"
  ```

  Without `LIBCLANG_PATH`, the `whisper-rs-sys` build fails with
  `Unable to find libclang`.
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

This build talks to Anthropic, OpenAI-compatible, or Vertex cloud backends, and
serves local GGUF models through the embedded `llama-server`. On a source build
that engine has to be on your `PATH`, covered in the last section.

## Step 2: install the SDK

Agents are Python. Install the `apollia` package in editable mode into the same
interpreter the runtime uses.

Create a virtual environment first. Homebrew, Debian and Fedora ship Python as an
externally managed environment (PEP 668): installing into it directly stops with
`error: externally-managed-environment`, and the runtime loads agents from the
interpreter it finds on `PATH`, so the environment you activate here is the one it
will use.

```sh
python3 -m venv .venv
source .venv/bin/activate
pip install -e ./sdk
```

On Windows, activate with `.venv\Scripts\activate` instead.

Keep this environment activated in every terminal where you run `apollia-os`. If
you prefer a system-wide install and accept the consequences, `pip install
--break-system-packages -e ./sdk` is the escape hatch, not the recommended path.

## Step 3: configure a model backend

An agent that generates text needs a backend. Pick one path.

`llm setup --local` writes straight to the local database and works offline.
Every other `llm` subcommand, including `backends create`, `reload` and `status`,
talks to the daemon. If you take the cloud path, or want to check with `llm
status`, start the daemon first with step 4 and come back here.

<!-- claim:cloud-llm-auth-is-api-key-only -->
Cloud: register a backend with an API key. That is the only way a cloud provider
authenticates; there is no OAuth flow for one.

```sh
apollia-os llm backends create prod --provider anthropic \
  --model claude-sonnet-4-20250514 --api-key "$ANTHROPIC_API_KEY" --default
```

`--api-key` also accepts a `${VAR}` form, resolved from the environment at
startup, so the key need not sit in `apollia.toml`.

Use your provider's current model id for `--model`; the value above is only an
example.

Local: point the runtime at a `.gguf` file on your machine. This registers the
backend; the daemon serves it through the embedded `llama-server` (see the last
section for the `llama-server` requirement).

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

<!-- claim:daemon-binds-tcp-by-default -->
On its first run the runtime creates its data directory at `~/.apollia`. It
listens on a Unix socket (`~/.apollia/runtime.sock` by default, set to `0600`
after binding) and on
`127.0.0.1:7771`. `apollia-os start` always binds TCP; `--port` chooses the
number, and omitting it takes the default 7771 rather than leaving the port
closed. On first boot it writes an API token to `~/.apollia/api-token`;
TCP callers must present it as a bearer credential, while the Unix socket is
local-trust and needs none. Leave this terminal running and open a second one for
the next steps.

On Windows there is no Unix socket. The runtime serves a named pipe instead,
`\\.\pipe\apollia-runtime-<user>`, and the command line opens it without you
naming anything. TCP is bound there too, and the pipe is not local-trust: it
carries the same bearer token as TCP, because a pipe takes a default security
descriptor rather than the socket's `0600`.

## Step 5: run an agent

The repository ships a no-LLM `echo` agent that runs on any machine. Install it,
enable it so the runtime loads it, then send it a task:

```sh
apollia-os agent install clients/examples/echo_agent.py --skip-tests
apollia-os agent enable echo
apollia-os run echo "hello from Apollia"
```

Without the `enable` step, `run` fails with `agent not found: echo` and a hint listing the install, enable and load sequence.

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

For local inference inside the dev app, make sure `llama-server` is on your `PATH`
(next section); the daemon the app embeds serves local GGUF models through it. See
[Get the most from local inference](/how-to/accelerate-local-inference).

## Build a release desktop bundle

The `just` recipes below produce a distributable desktop installer (`.dmg` on
macOS, `.deb`/`.AppImage` on Linux, `.msi`/`.exe` on Windows). They run
`bundle-cli.sh`, which stages the Python runtime, the `apollia-os` CLI, the
speech-to-text runners, and a pinned `llama-server` binary.

Each recipe accepts two optional arguments:

| Argument | Role | Default (macOS / Linux / Windows) |
|---|---|---|
| `target` | Rust triple passed to `cargo tauri build` | `aarch64-apple-darwin` / `x86_64-unknown-linux-gnu` / `x86_64-pc-windows-msvc` |
| `runners` | Space-separated list of runner backends to build and bundle | `cpu metal` / `cpu` / `cpu` |

The `runners` value controls two things:

1. Which `apollia-runner-{backend}` sidecars are compiled and copied into the
   bundle.
2. Which prebuilt `llama-server` asset is downloaded. The script picks the
   first GPU backend in the list (`metal`, `cuda`, `rocm`, or `vulkan`); if
   none is present it falls back to CPU. Setting
   `APOLLIA_DESKTOP_LLAMA_BACKEND` overrides that choice and the list is not
   consulted, which is how the release pipeline builds a CUDA engine next to
   CPU speech-to-text runners.

`cpu` is always included as a universal fallback. Add one GPU backend that
matches your hardware:

| Hardware | Typical `runners` value | Notes |
|---|---|---|
| Apple Silicon | `cpu metal` | Default macOS preset |
| NVIDIA (CUDA 12+) | `cpu cuda` | On Windows, LLM and STT both use CUDA. On Linux this value fails the bundle: the pinned upstream release ships no Linux CUDA `llama-server`, the fetch exits and the script stops on `could not bundle llama-server (cuda)`. Build one and pass `LLAMA_SERVER_DIR`, which is what the release pipeline does for the Linux `-cuda` bundle |
| AMD Radeon / Intel Arc | `cpu vulkan` | LLM on GPU; STT stays CPU (`whisper-rs` has no Vulkan backend) |
| AMD Pro / Instinct + HIP SDK | `cpu rocm` | LLM and STT on ROCm where supported |

Platform presets:

```sh
# macOS Apple Silicon, Metal + CPU fallback (defaults)
just release-macos

# Linux x86_64, CPU only (default)
just release-linux

# Windows x86_64, CPU only (default)
just release-windows
```

Override the target and/or runners on any preset:

```sh
# Windows with Vulkan LLM (AMD / Intel / fallback NVIDIA)
just release-windows runners="cpu vulkan"

# Windows with CUDA (NVIDIA)
just release-windows runners="cpu cuda"

# Linux with Vulkan
just release-linux runners="cpu vulkan"

# macOS with a custom runner set
just release-macos runners="cpu metal"
```

For a triple and runner set not covered by a preset, use the generic recipe:

```sh
just release-desktop x86_64-pc-windows-msvc "cpu vulkan"
just release-desktop x86_64-unknown-linux-gnu "cpu rocm"
just release-desktop aarch64-apple-darwin "cpu metal"
```

The bundled `llama-server` engine is fetched from the pinned upstream
llama.cpp release, which publishes builds for macOS (arm64 and x86-64), Linux
x86-64 (CPU, Vulkan, ROCm), Linux arm64 (CPU) and Windows x86-64 (CPU, CUDA,
Vulkan). For any other couple, build llama.cpp yourself and pass
`LLAMA_SERVER_DIR=<bin dir>`; the recipe then bundles your build.

On Windows, export `LIBCLANG_PATH` and `CMAKE_MSVC_RUNTIME_LIBRARY` in the
same shell before running any of these recipes (see the Windows prerequisites
above). On Linux, the speech-to-text runner additionally needs `clang` and
`cmake` in your package manager.

The bundle lands under `target/<triple>/release/bundle/` (for example
`target/x86_64-pc-windows-msvc/release/bundle/msi/` on Windows).

## Local GGUF inference

Local models run through an embedded `llama-server` (upstream llama.cpp) that the
daemon spawns and supervises over its OpenAI-compatible HTTP API, with native tool
calling (`--jinja`) and continuous batching. The provider name stays `llama-cpp`.

A packaged desktop build stages `llama-server` automatically, next to the
speech-to-text runners, so nothing is needed there. On a source build the daemon
looks for `llama-server` on your `PATH`. Provide one of:

an upstream install that puts `llama-server` on your `PATH`, for example
`brew install llama.cpp` on macOS or a llama.cpp build on Linux.

The repository also has a `just llama-server` recipe. It does **not** satisfy this
prerequisite: it expects `llama-server` to be on the `PATH` already, and it starts
a separate server on port 8899 that the daemon does not talk to. It is a
developer bench for comparing against a hand-tuned server, not an install step.

If a local backend is configured but no `llama-server` is reachable, LLM calls
fail with a `503 Service Unavailable` and a `BackendUnavailable` reason; put the
engine on your `PATH` to resolve it.

There is no download command for models. Obtain a `.gguf` file yourself (for
example from a model hub) and place it in `~/.apollia/models/`, then point a local
backend at it with `apollia-os llm setup --local --model <path.gguf>`. The `model`
subcommands (`list`, `search`, `show`, `hardware`, `delete`) inspect and manage the
models already present; see the [CLI reference](/reference/cli).

Speech-to-text is a separate, optional component. The `apollia-runner` sidecar,
built with a `local-*` feature (`local-metal`, `local-cpu`, `local-cuda`,
`local-rocm`, `local-vulkan`), runs whisper out of process; it no longer serves
LLM inference. A packaged build bundles it, and from source you build it only if
you want local dictation.

## Next steps

- Write and run your own agent: [Your first agent](/tutorials/your-first-agent).
- Run Apollia as a managed service: [Deploy in production](/how-to/deploy-in-production).
- Every flag on every command is in the [CLI reference](/reference/cli).
