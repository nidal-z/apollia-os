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

Apollia ships **without** an OAuth client for the Google and Microsoft
connectors, and no published build embeds one. You register your own
application with the provider and give Apollia its credentials. The supported
way to do that is Settings → OAuth integrations, which writes
`~/.apollia/oauth-clients.toml`; see
[Connect Google Workspace](/operator-help/integrations/connecter-google-workspace)
and
[Connect Microsoft 365](/operator-help/integrations/connecter-microsoft-365).

<!-- claim:oauth-client-resolution-order -->
The variables below are the third way in, ahead of that file, for a shell
session, a CI job, or a host with no interface. They are read at process start,
so they only reach an Apollia launched from the shell that exported them.
Resolution order for each credential: environment variable, then
`oauth-clients.toml`, then the build-time constant.

| Variable | Effect |
|---|---|
| `APOLLIA_GOOGLE_CLIENT_ID` | Google client id. Required to connect Google. |
| `APOLLIA_GOOGLE_CLIENT_SECRET` | Paired secret. Also required: Google's Desktop client type demands it at the token endpoint, PKCE notwithstanding. |
| `APOLLIA_GOOGLE_API_KEY` | API key for the Google calls that use one rather than OAuth (Drive Picker). |
| `APOLLIA_MICROSOFT_CLIENT_ID` | Microsoft application (client) id. Required to connect Microsoft. |
| `APOLLIA_MICROSOFT_CLIENT_SECRET` | Paired secret. Leave unset: a Microsoft public client carries none, and sending one makes the exchange fail. |
| `APOLLIA_MICROSOFT_API_KEY` | API key, same role as above. Unused today. |
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

`APOLLIA_BUILD_*` variables are read at compile time, not at runtime; setting
one before launching Apollia does nothing.

They are a hook for rebuilding Apollia from source against your own registered
application, a fleet deployment being the obvious case: set them in the build
environment and the resulting binary carries those credentials, so the machines
it lands on need no per-host configuration. **No Apollia release sets them**, so
in every published build the compiled-in value is empty and the two runtime
sources above are the only ones that resolve.
