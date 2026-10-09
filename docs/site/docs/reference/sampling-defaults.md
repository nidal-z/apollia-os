---
sidebar_position: 8
title: LLM sampling defaults
---

# LLM sampling defaults

<!-- claim:sampling-only-temperature-reaches-the-backend -->

A request to a model carries its sampling settings as optional fields. A field
that is set is sent to the backend; a field left unset is not sent, and the
provider or `llama-server` applies its own default. This page lists the fields,
the names they travel under, and the levels of defaults that fill the fields a
call leaves unset.

## Request fields

Every backend is reached through the OpenAI-compatible chat completions
protocol, the embedded `llama-server` included.

| Field | Sent as | Protocol |
|---|---|---|
| `temperature` | `temperature` | OpenAI |
| `max_tokens` | `max_tokens` | OpenAI |
| `seed` | `seed` | OpenAI |
| `top_p` | `top_p` | OpenAI |
| `presence_penalty` | `presence_penalty` | OpenAI |
| `frequency_penalty` | `frequency_penalty` | OpenAI |
| `stop` | `stop` | OpenAI. An empty list sends nothing. |
| `top_k` | `top_k` | `llama-server` extension |
| `min_p` | `min_p` | `llama-server` extension |
| `repeat_penalty` | `repeat_penalty` | `llama-server` extension |
| `thinking`, `reasoning_effort` | `chat_template_kwargs` | `llama-server` extension, read by the chat template of the model |

The three sampling extensions are written into the request body under the
names `llama-server` reads, and they are sent to every backend when set, not
only to the embedded engine. A remote server that does not know `top_k`,
`min_p` or `repeat_penalty` does what it does with an unknown field, refusal
included. Leave them unset for such a server.

## Per call, from a workflow step

`ctx.llm.complete` takes every field of the table except `reasoning_effort` as
a keyword:

```python
await ctx.llm.complete(messages, temperature=0.3, top_p=0.9, seed=7, stop=["END"])
```

A call of a workflow step belongs to no agent, so none of the levels below
applies to it: what the call sets is what is sent. One bound is added:
`complete` sends `max_tokens = 4096` when the call sets none. `ctx.llm.chat`
and `ctx.llm.stream` take `temperature`, `max_tokens` and `seed` only.

## Levels of defaults for an agent

Each model call of an agent turn starts from request settings resolved field
by field. A level wins over the ones below it; a field it leaves unset keeps
the value of the level below.

| Level | Source | Fields |
|---|---|---|
| 4. Conversation | The settings typed for one conversation, which hold for it alone | `temperature`, `max_tokens` |
| 3. Preset | The preset chosen for the conversation, else the one chosen for the agent, else the default preset of the model of the agent. A preset belongs to one model and counts only when the call runs on it | `temperature`, `max_tokens`, `top_p`, `top_k`, `min_p`, `presence_penalty`, `repeat_penalty`, `stop` |
| 2. Package | `[agents.defaults.model.request]` in the [package manifest](/reference/package-manifest) | `temperature`, `top_p`, `top_k`, `repetition_penalty`, `max_tokens`, `seed` |
| 1. Model | The defaults its publisher recommends, see the next section | `temperature`, `top_p`, `top_k`, `min_p`, `presence_penalty`, `repetition_penalty` |

`temperature` has one more rule. When neither the conversation nor the preset
sets it, a call that offers tools to the model uses `[chat]
tool_turn_temperature` (0.3 when the key is unset), ahead of the package and
the model levels. A lower value makes tool selection steadier. See
[Configuration](/reference/configuration).

## Model defaults

The model level is itself resolved from three sources, field by field, the
first that answers winning:

1. The model card copied next to the model file when it was downloaded.
2. `~/.apollia/models/sampling-defaults.json`. A download from Hugging Face
   reads the `generation_config.json` the model publishes and writes the
   values it finds there, under the file name of the model. The file can also
   be edited by hand.
3. A table of twelve model families embedded in the binary, matched on the
   architecture the GGUF header declares and on the name of the model.

The file is a flat map from model file name to fields:

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

A model none of the three knows gets no default from this level.

## Reproducibility

`seed` reaches the backend, from a call or from the package level. Whether two
runs with the same seed produce the same text depends on the engine and on the
server behind the backend: the runtime sends the value and guarantees nothing
beyond that. Without a seed, two runs of the same prompt on the same model can
differ.

What is recorded is not the generation itself. The hash-chained journal holds
the tool-call and LLM-call entries of a run, a conversation included. See
[Audit and verify a run](/how-to/audit-and-verify).
