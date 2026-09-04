---
sidebar_position: 6
title: Environment variables
---

# Environment variables

What the runtime reads from its environment, and what it does with it. The
tables below cover the `APOLLIA_*` names Apollia defines. Apollia also reads
standard variables it does not own, where their usual meaning applies:
`XDG_CONFIG_HOME` and `EDITOR` when the CLI resolves or edits `apollia.toml`,
`NO_COLOR` for terminal output, `PATH` when it looks for a binary, `TZ`, and
`USERNAME` on Windows to derive the named pipe. A cloud backend reads whatever
variable its `api_key_env` names.

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
| `APOLLIA_LLAMA_SERVER_BIN` | the engine bundled with the artifact | Absolute path to a `llama-server` binary, which takes precedence over the bundled one. The way to run a build of your own, a CUDA build on Linux among them. |
| `APOLLIA_LLAMA_MAX_LOADED` | `1` | How many models may stay resident at once. Each extra resident model holds its weights in memory until it is unloaded, so raising the ceiling is an explicit act. A zero or unparseable value keeps the default. |
| `APOLLIA_LLAMA_N_CTX` | `32768` | Context window in tokens. The default is a fixed value, not read from the model. |
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

The GGUF the engine loads has no environment override, deliberately. It is
state owned by the model switch in the interface, and an ambient variable
pinning it would defeat every switch made there.

## Secret storage

| Variable | Default | Effect |
|---|---|---|
| `APOLLIA_TOKEN_STORAGE` | `keyring` | `keyring` uses the OS keychain. `file` stores secrets as `age`-encrypted files under `~/.apollia/secrets/`, for a headless Linux host where no keyring daemon is reachable. Connector OAuth tokens are outside this switch: they are written to the OS keyring whatever the value here, so a host with no reachable keyring cannot hold them. |
| `APOLLIA_TOKEN_PASSPHRASE` | none | Passphrase for the `file` backend. **Mandatory when `APOLLIA_TOKEN_STORAGE=file`**: startup fails fast without it, rather than falling back to something weaker. |

## Connector OAuth clients

The two connectors differ, and the variables below behave accordingly.

**Microsoft** ships ready to use: Apollia registers a public client application
and embeds its identifier, so nothing has to be configured. A public client
holds no secret, which is what makes shipping the identifier harmless; see
[Connect Microsoft 365](/operator-help/integrations/connect-microsoft-365).

**Google** ships **without** a client, and no published build embeds one. You
register your own application and give Apollia its credentials, because Google
requires a verified consent screen and a client secret that a distributed binary
cannot hold. See
[Connect Google Workspace](/operator-help/integrations/connect-google-workspace)
and [Set up a Google OAuth client](/how-to/set-up-a-google-oauth-client).

Either way, the supported route is Settings → OAuth integrations, which writes
`~/.apollia/oauth-clients.toml`.

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
| `APOLLIA_MICROSOFT_CLIENT_ID` | Microsoft application (client) id. **Optional**, and it overrides the identifier Apollia ships. Set it only to point the connector at your own Entra registration; exporting it empty leaves Apollia's own identifier in place. |
| `APOLLIA_MICROSOFT_CLIENT_SECRET` | Paired secret. Leave unset: a Microsoft public client carries none, and sending one makes the exchange fail. |
| `APOLLIA_MICROSOFT_API_KEY` | API key, same role as above. Unused today. |
| `APOLLIA_FIGMA_CLIENT_ID` | Client id for the Figma connector. |

## Diagnostics

| Variable | Default | Effect |
|---|---|---|
| `APOLLIA_PERF_TRACE` | unset | Path to a file receiving a per-turn performance record. Unset means no file is written and no provenance is gathered; the summary is emitted at `INFO` either way. |
| `RUST_LOG` | `apollia=info` | Standard `tracing` filter. `apollia=trace` is what makes `[llm.observability] debug_log_prompt` visible; see [Configuration](/reference/configuration). |

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
it lands on need no per-host configuration.

**No Apollia release sets them.** For Google that means the compiled-in value is
empty in every published build, and the two runtime sources above are the only
ones that resolve. For Microsoft the compiled-in value is not empty even so: it
comes from a constant in the source, not from these variables, which is why
setting `APOLLIA_BUILD_MICROSOFT_CLIENT_ID` is only ever needed to replace
Apollia's registration at build time.
