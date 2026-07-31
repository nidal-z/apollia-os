---
sidebar_position: 6
title: Environment variables
---

# Environment variables

What the runtime reads from its environment, and what it does with it. Anything
not listed here is either build-time or test-only.

Most configuration belongs in `apollia.toml`, see
[Configuration](/reference/configuration). Environment variables cover three
cases the file cannot: a secret you do not want on disk, a per-launch override,
and a path that depends on the machine.

## Local inference engine

Read at every engine start. See
[Accelerate local inference](/how-to/accelerate-local-inference) for what to
tune and why.

| Variable | Default | Effect |
|---|---|---|
| `APOLLIA_LLAMA_SERVER_BIN` | resolved from `PATH` | Absolute path to the `llama-server` binary. |
| `APOLLIA_LLAMA_MODEL_PATH` | from the configured backend | Overrides the GGUF the engine loads. |
| `APOLLIA_LLAMA_MAX_LOADED` | see the source default | How many models may stay resident at once. |
| `APOLLIA_LLAMA_N_CTX` | model-derived | Context window in tokens. |
| `APOLLIA_LLAMA_N_GPU_LAYERS` | `999` | Layers offloaded to the GPU; `0` forces CPU. |
| `APOLLIA_LLAMA_N_BATCH` | engine default | Logical batch size. |
| `APOLLIA_LLAMA_N_UBATCH` | engine default | Physical micro-batch size. |
| `APOLLIA_LLAMA_N_PARALLEL` | `1` | Decode slots served concurrently. |
| `APOLLIA_LLAMA_CONT_BATCHING` | `true` | Continuous batching. |
| `APOLLIA_LLAMA_CACHE_TYPE_K` | engine default | KV cache quantization, keys. |
| `APOLLIA_LLAMA_CACHE_TYPE_V` | engine default | KV cache quantization, values. |
| `APOLLIA_LLAMA_FLASH_ATTN` | `on` | Flash attention mode. |
| `APOLLIA_LLAMA_CACHE_REUSE` | engine default | Prefix-reuse threshold. |
| `APOLLIA_LLAMA_METRICS` | `false` | Exposes the engine's metrics endpoint. |
| `APOLLIA_LLAMA_EXTRA_ARGS` | empty | Extra flags passed through verbatim. |

## Secret storage

| Variable | Default | Effect |
|---|---|---|
| `APOLLIA_TOKEN_STORAGE` | `keyring` | `keyring` uses the OS keychain. `file` stores secrets as `age`-encrypted files under `~/.apollia/secrets/`, for a headless Linux host where no keyring daemon is reachable. |
| `APOLLIA_TOKEN_PASSPHRASE` | none | Passphrase for the `file` backend. **Mandatory when `APOLLIA_TOKEN_STORAGE=file`**: startup fails fast without it, rather than falling back to something weaker. |

## Connector OAuth clients

Apollia ships with its own OAuth client for the Google and Microsoft connectors.
Set these only to run your own registered application, which is what Expert Mode
means.

| Variable | Effect |
|---|---|
| `APOLLIA_GOOGLE_CLIENT_ID` | Overrides the compiled-in Google client id. |
| `APOLLIA_GOOGLE_CLIENT_SECRET` | Paired secret. |
| `APOLLIA_GOOGLE_API_KEY` | API key for the Google calls that use one rather than OAuth. |
| `APOLLIA_MICROSOFT_CLIENT_ID` | Overrides the compiled-in Microsoft client id. |
| `APOLLIA_MICROSOFT_CLIENT_SECRET` | Paired secret. |
| `APOLLIA_MICROSOFT_API_KEY` | API key, same role as above. |
| `APOLLIA_FIGMA_CLIENT_ID` | Client id for the Figma connector. |

## Diagnostics

| Variable | Default | Effect |
|---|---|---|
| `APOLLIA_PERF_TRACE` | unset | Path to a file receiving a per-turn performance record. Unset means no file is written and no provenance is gathered; the summary is emitted at `INFO` either way. |
| `APOLLIA_MCP_PROTOCOL_VERSION` | pinned in the code | Overrides the MCP protocol revision announced to a server. For probing a server that pins a different revision, not for normal use. |
| `RUST_LOG` | `apollia=info` | Standard `tracing` filter. `apollia=trace` is what makes `[llm.observability] debug_log_prompt` visible; see [Configuration](/reference/configuration). |

## Bundled companion agent

Overrides used when developing the companion agent shipped with the desktop
application. They point the runtime at a working copy instead of the embedded
copy.

`APOLLIA_GUIDE_PY`, `APOLLIA_GUIDE_TOML`, `APOLLIA_GUIDE_CAPABILITIES_MD`,
`APOLLIA_GUIDE_TUTORIALS_MD`, `APOLLIA_GUIDE_VERSION`.

## Desktop automation

`APOLLIA_AUTOMATION`, `APOLLIA_AUTOMATION_OUT` and
`APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE` drive the development-only gestural test
harness. They have no effect in a release build, where the harness is compiled
out.

## Not read by the runtime

`APOLLIA_BUILD_*` variables are consumed by the release pipeline at compile time
to bake in default OAuth client ids. Setting one at runtime does nothing.
