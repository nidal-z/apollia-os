# apollia-runner

Sidecar runner pour l'inférence LLM/STT locale d'Apollia OS.

Process enfant spawné par le daemon `apollia-os` au boot. Contient `llama-cpp-2` + `whisper-rs` compilés avec un seul backend GPU à la fois (CUDA, ROCm, Vulkan, Metal, ou CPU). Communique avec le daemon via HTTP/JSON sur loopback TCP.

Pourquoi ce design : voir [ADR-113](../../docs/adr/ADR-113-multi-runner-sidecar-architecture.md).

## Build

Le crate produit 5 binaires différents selon la feature Cargo activée :

```sh
cargo build --release -p apollia-runner --features local-cpu
cargo build --release -p apollia-runner --features local-cuda
cargo build --release -p apollia-runner --features local-rocm
cargo build --release -p apollia-runner --features local-vulkan
cargo build --release -p apollia-runner --features local-metal   # macOS uniquement
```

Le binaire produit s'appelle `apollia-runner` (sans suffixe). Le packaging final (cf. `release.yml`) le renomme `apollia-runner-{backend}` selon la feature.

## Test standalone

```sh
cargo run --release -p apollia-runner --features local-cpu
# stdout: "READY 38492\n"
# stderr: logs JSON Lines

curl http://127.0.0.1:38492/handshake | jq .
```

## Liens

- Spec protocole : [docs/internal/architecture/IPC-PROTOCOL.md](../../docs/internal/architecture/IPC-PROTOCOL.md)
- Structure crate : [docs/internal/architecture/CRATE-LAYOUT.md](../../docs/internal/architecture/CRATE-LAYOUT.md)
