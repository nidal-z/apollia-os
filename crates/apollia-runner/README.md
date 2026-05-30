# apollia-runner

Sidecar runner for Apollia OS local LLM and STT inference.

Child process spawned by the `apollia-os` daemon at boot. Bundles `llama-cpp-2` and `whisper-rs` compiled with a single GPU backend at a time (CUDA, ROCm, Vulkan, Metal, or CPU). Communicates with the daemon over HTTP/JSON on a loopback TCP port.

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

## Standalone test

```sh
cargo run --release -p apollia-runner --features local-cpu
# stdout: "READY 38492\n"
# stderr: JSON Lines logs

curl http://127.0.0.1:38492/handshake | jq .
```

## References

- Protocol spec: [docs/internal/architecture/IPC-PROTOCOL.md](../../docs/internal/architecture/IPC-PROTOCOL.md)
- Crate layout: [docs/internal/architecture/CRATE-LAYOUT.md](../../docs/internal/architecture/CRATE-LAYOUT.md)
