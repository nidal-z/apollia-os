---
sidebar_position: 9
title: Get the most from local inference
---

# Get the most from local inference

Local LLM inference is served by an embedded `llama-server` (upstream
llama.cpp) that the daemon spawns and supervises over its OpenAI-compatible HTTP
API. This is the built-in and only local engine: there is no separate process to
install, run, or point Apollia at. It ships prebuilt inside the desktop app, and
on a source build the daemon finds `llama-server` on your `PATH` (see
[Install and run the runtime](/how-to/install-and-run#local-gguf-inference)).

Two capabilities that used to require an extra, hand-run server are now on by
default, because the embedded engine is that server:

- **Continuous batching.** The engine decodes several sequences in the same GPU
  pass, so concurrent and batch requests share the hardware instead of
  serializing one behind another. Nothing to enable.
- **Native tool calling.** The engine is driven with `--jinja`, so tool calls go
  through the model's own chat template rather than a bespoke grammar path. Local
  models call your tools reliably, with no tuning on your side.

Tracking upstream llama.cpp also widens model coverage: newer architectures land
in the engine as they land upstream.

## Configure a local backend

Register a `.gguf` model as a local backend and let the daemon serve it. The
provider name is `llama-cpp`:

```sh
apollia-os llm setup --local --model /path/to/model.gguf
apollia-os llm reload
apollia-os llm status
```

The daemon starts the embedded `llama-server` for that model on demand and routes
inference to it. SSE streaming and tool calls travel the same path, already wired
and tested.

## Get good throughput

The engine handles the mechanics (GPU offload, batching, KV cache). The choices
that move throughput are upstream of it:

- **Pick a model sized to your hardware.** A mixture-of-experts (MoE) model
  activates only a fraction of its parameters per token, so it can beat a dense
  model of similar quality on both speed and batch throughput. Prefer a
  quantization that leaves headroom for the KV cache.
- **Serve one model per server process.** The engine loads a single-file GGUF
  model. Switching the default backend switches which model the daemon serves.
- **Let concurrency ride the batch.** Because continuous batching is always on,
  several agents (or several steps of one orchestrated run) can decode together
  without you provisioning extra slots by hand.

## Developer: run a tuned `llama-server`

On a source build the daemon uses the `llama-server` it finds on your `PATH`
rather than a bundled binary. The repository ships a recipe to start one for
local testing:

```sh
just llama-server /path/to/model.gguf
```

That recipe runs the upstream binary, so the usual llama.cpp options apply when
you experiment locally: total context (`-c`) split across parallel slots (`-np`),
GPU offload (`-ngl`), flash attention (`--flash-attn on`), and a quantized KV
cache (`-ctk q8_0 -ctv q8_0`, which needs flash attention). These are upstream
llama.cpp flags, useful for probing what your hardware sustains before you settle
on a model and quantization.

## Related

- [Install and run the runtime](/how-to/install-and-run) for the build and the
  `PATH` requirement on a source build.
- [Deploy in production](/how-to/deploy-in-production) for serving on a server.
- The [CLI reference](/reference/cli) for the `llm` commands.
