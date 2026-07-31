---
sidebar_position: 6
title: LLM sampling defaults
---

# LLM sampling defaults

<!-- claim:sampling-only-temperature-reaches-the-backend -->

**One sampling parameter reaches a model: `temperature`.** A request carries
`temperature`, `max_tokens`, `seed`, `model`, `messages`, `tools` and an optional
grammar, and nothing else. `top_p`, `top_k` and `repetition_penalty` are not
fields of a request; a backend never receives them. When a caller passes no
`temperature`, the runtime sets none and the provider or `llama-server` applies
its own default.

That is the whole of what governs sampling today. The rest of this page describes
machinery that exists and is not yet consumed, listed here because it is visible
on disk and would otherwise look like a setting that works.

## Setting temperature

Per call, through the SDK:

```python
await ctx.llm.complete(messages=..., temperature=0.3)
```

Per chat session, `[chat] tool_turn_temperature` applies to a turn that
advertises tools, where a lower value makes tool selection steadier. See
[Configuration](/reference/configuration).

## What exists but is not wired

Downloading a GGUF from HuggingFace reads the model's published
`generation_config.json` and writes the hyperparameters it finds into
`~/.apollia/models/sampling-defaults.json`, a flat map from model key to fields:

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

The runtime also embeds a curated table of twelve entries covering the same
families. **Neither is read at inference time.** The resolver that would consult
them has no caller outside test code, so editing the file changes nothing today.
It is written, not applied.

## Reproducibility

**A run is not reproducible.** `ctx.llm.complete`, `chat` and `stream` accept a
`seed`, and the value is carried as far as the request struct, but no backend
reads it: there is no occurrence of `seed` in any backend implementation. Passing
one is accepted and has no effect.

Two runs of the same prompt on the same model can therefore differ. What is
reproducible is the record of what happened, not the generation itself: the audit
trail and the event log capture each call, its inputs and its result. See
[Audit, verify and roll back a run](/how-to/audit-verify-rollback).
