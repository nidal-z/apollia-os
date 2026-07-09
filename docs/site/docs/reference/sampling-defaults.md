---
sidebar_position: 6
title: LLM sampling defaults
---

# LLM sampling defaults

Apollia resolves the sampling parameters (`temperature`, `top_p`, `top_k`,
`repetition_penalty`) for a local model at inference time, per model, so each
family runs with sensible values without hand-tuning. This page documents how a
value is chosen, how to override it, and how reproducibility works. It applies to
the local llama.cpp backend served by the `apollia-runner` sidecar; cloud backends
use their provider's own defaults.

For the caller-facing API that accepts a per-call `temperature` and `seed`, see
[`ctx.llm`](/reference/sdk/llm). For file-based configuration in general, see
[Configuration](/reference/configuration).

## Resolution precedence

For each field, the first source that supplies a value wins. Resolution is
field-by-field: a user override that sets only `temperature` still lets `top_p` and
`top_k` come from the curated table.

1. **Caller request.** A `temperature` passed on the call (for example
   `ctx.llm.complete(..., temperature=...)`) takes precedence.
2. **User override file.** `~/.apollia/models/sampling-defaults.json`, keyed by
   model.
3. **Curated table.** A small built-in table matched against the model's family.
4. **Hard fallback.** When no source supplies a field.

The hard-fallback values are `temperature = 0.7`, `top_p = 0.95`, `top_k = 40`, and
`repetition_penalty = 1.1`. They are constants applied at the point the request is
assembled, not a configuration surface.

One caveat worth knowing: at the site where the request payload is assembled, only
`temperature` reads the caller's per-call value; `top_p`, `top_k`, and
`repetition_penalty` come from the resolved defaults or the hard fallback. To pin
those three for a given model, set them in the override file.

## The curated table

The runtime ships a curated table of sampling defaults, sourced from the official
`generation_config.json` published by each model's authors. It holds twelve entries
across six families: Qwen, Llama, Mistral and Mixtral, Phi, Gemma, and DeepSeek.
Entries are matched by the model's architecture and name, with more specific
entries taking precedence (for example a Qwen "thinking" variant before the generic
Qwen entry). The stored values are the numeric hyperparameters only.

## User overrides

To override defaults for a specific model, write
`~/.apollia/models/sampling-defaults.json`. It is a flat JSON map from a model key
(a GGUF filename, a repo id, or any other hint) to a set of sampling fields. The
file is read on each call, so edits take effect on the next inference with no
restart.

```json
{
  "Qwen3-30B-A3B-Q4_K_M.gguf": {
    "temperature": 0.6,
    "top_p": 0.95,
    "top_k": 20,
    "repetition_penalty": null
  }
}
```

Any field you omit falls through to the curated table, then the hard fallback.
Indexing by filename lets you tune each quantization of the same repository
independently.

## Auto-fetch on download

When you download a model through Apollia, the runtime makes a best-effort fetch of
the model's `generation_config.json` from Hugging Face and persists the numeric
sampling values into `~/.apollia/models/sampling-defaults.json`, keyed by the GGUF
filename. If the file is absent upstream the step is logged and skipped; it never
fails the download.

## Reproducibility

Sampling is stochastic by default so an agent can explore different angles over
time. To reproduce a run, pass a `seed` on the call. When no seed is given, one is
derived from the system clock. A `temperature` of `0` selects greedy decoding,
which is deterministic and ignores the seed.

## Related

- [`ctx.llm`](/reference/sdk/llm) for the completion and streaming API.
- [Configuration](/reference/configuration) for the rest of the runtime
  configuration surface.
