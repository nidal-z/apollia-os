---
sidebar_position: 9
title: Accelerate local inference
---

# Accelerate local inference with llama-server

The embedded runner (`apollia-runner-*`) is the default local inference path and
covers most workloads. It ships prebuilt with the desktop app, so there is
nothing extra to install there; on a server you build it once and co-locate it
next to the `apollia-os` binary (see
[Install and run the runtime](/how-to/install-and-run#optional-enable-local-gguf-inference)). This guide adds an optional, higher
throughput path for two cases: serving many concurrent requests, and speculative
decoding for lower latency. It stays 100% local, is config-only (no engine code),
and does not replace the embedded runner, it accelerates it where the deployment
allows.

You run one extra local process, `llama-server` from llama.cpp, and point Apollia
at it over its OpenAI-compatible API. It is an external dependency the operator
takes on, not the default distribution.

## When to use it

- **Concurrent or batch throughput.** The embedded runner drives llama.cpp with
  independent contexts that serialize on a single GPU, so extra slots add no
  measured throughput. `llama-server`'s continuous batching decodes several
  sequences in the same GPU pass. Internal measurement, indicative only: up to
  ~2.4x aggregate throughput on a 30B MoE model at 8 concurrent requests.
- **Unit latency.** `llama-server` exposes speculative decoding
  (`--spec-draft-n-max`), which the embedded runner does not at the pinned
  llama-cpp version.

## 1. Install llama-server

```sh
# macOS (Metal)
brew install llama.cpp        # -> /opt/homebrew/bin/llama-server

# Linux / build from source: github.com/ggml-org/llama.cpp
#   cmake -B build -DGGML_CUDA=ON && cmake --build build
```

## 2. Run the server

Base recipe (continuous batching + flash attention):

```sh
llama-server \
  -m ~/.apollia/models/<model>.gguf \
  -ngl 999          `# all layers on the GPU (Metal/CUDA)` \
  -c 16384          `# TOTAL context; per slot = c / np` \
  -np 8             `# parallel slots` \
  -cb               `# continuous (dynamic) batching` \
  --flash-attn on \
  --host 127.0.0.1 --port 8080
```

`-c` is the total context; each slot gets `c / np`. For 8 slots of 2048 tokens
each, use `-c 16384 -np 8`. Optional: quantize the KV cache to fit more context or
more slots (`-ctk q8_0 -ctv q8_0`, requires `--flash-attn on`).

## 3. Point Apollia at it (config-only)

`llama-server` exposes an OpenAI-compatible API. Apollia consumes it through its
existing OpenAI-compatible backend, no API key needed for a local server.

```sh
apollia-os llm backends create local-fast \
  --provider openai \
  --model <model-name> \
  --base-url http://127.0.0.1:8080/v1 \
  --default

apollia-os llm reload        # switch the router without restarting the daemon
```

The backend name (`local-fast`) is positional; `--base-url` feeds
`config_json.base_url`; `--default` makes it the default backend. SSE streaming and
tool calls go through the same OpenAI-compatible path, already wired and tested.

## 4. Speculative decoding (optional)

Speculative decoding runs a small "draft" model that proposes tokens the target
model verifies in one pass. The draft must share the target's tokenizer and vocab
(same model family), otherwise verification fails.

```sh
llama-server \
  -m ~/.apollia/models/<target>.gguf \
  -md ~/.apollia/models/<draft>.gguf \
  --spec-draft-n-max 8      `# tokens proposed per step` \
  -ngl 999 -c 16384 -np 8 -cb --flash-attn on --host 127.0.0.1 --port 8080
```

Rule of thumb: a draft ~10x smaller than the target, same publisher and generation.

## Caveats

- **External process.** `llama-server` is yours to supervise and restart. Apollia
  does not spawn it (unlike the embedded runner).
- **Speculative helps predictable text** (code, structured formats). Typical gain
  is a few tens of percent up to ~1.8x, and it can be neutral or negative on open
  chat with an already-fast model. Treat the numbers here as indicative.
- **Not a replacement.** This path accelerates deployments that accept an extra
  process; the embedded runner remains the zero-install default.

## Related

- [Install and run the runtime](/how-to/install-and-run) for the embedded runner.
- [Deploy in production](/how-to/deploy-in-production) for serving on a server.
- The [CLI reference](/reference/cli) for the `llm backends` commands.
