# apollia-runner

Sidecar runner for Apollia OS local speech-to-text (STT) inference.

Child process spawned by the `apollia-os` daemon at boot. Bundles `whisper-rs` compiled with a single GPU backend at a time (CUDA, ROCm, Vulkan, Metal, or CPU). Communicates with the daemon over HTTP/JSON on a loopback TCP port. Local LLM inference no longer runs here; it goes through the embedded `llama-server` (upstream llama.cpp), which the daemon supervises and speaks to over its OpenAI-compatible HTTP API.

## Build

The crate produces a different binary per Cargo feature:

```sh
cargo build --release -p apollia-runner --features local-cpu
cargo build --release -p apollia-runner --features local-cuda
cargo build --release -p apollia-runner --features local-rocm
cargo build --release -p apollia-runner --features local-vulkan
cargo build --release -p apollia-runner --features local-metal   # macOS only
```

The produced binary is named `apollia-runner` (no suffix). Final packaging (`release.yml`) renames it to `apollia-runner-{backend}` according to the feature.

On Windows, build through `just runner-release <backend>` rather than the bare `cargo build`: the recipe sources `host_runner_cmake_env`, which restores the optimisation flags the `cmake` crate strips from whisper.cpp on MSVC (a bare release build transcribed thirty times slower than the debug one, measured 2026-09-10) and, for `local-vulkan`, routes ggml's shader generator around a 260-character path limit. `local-vulkan` needs the Vulkan SDK (`VULKAN_SDK`) at build time.

## Standalone test

```sh
cargo run --release -p apollia-runner --features local-cpu
# stdout: "READY 38492\n"
# stderr: JSON Lines logs

curl http://127.0.0.1:38492/handshake | jq .
```

## References

- Crate source: `crates/apollia-runner/src/`
