---
title: Configure hybrid local + frontier routing
slug: /operator-help/installation/configure-hybrid-routing
sidebar_position: 7
---

# Configure hybrid local + frontier routing

> For any operator who wants to combine a fast local model for simple steps and a powerful cloud model for complex steps, with an automatic cost ceiling.

## Prerequisites

- At least one backend for the default side, a `.gguf` model served by the embedded `llama-server` engine. See [Download local models](telecharger-des-modeles-locaux.md). An Ollama server does not belong in that role: Apollia reaches it over HTTP like any other OpenAI-compatible endpoint, so it is a remote backend even when it runs on your own network.
- At least one cloud (frontier) backend declared. See [Connect a remote model](connecter-un-modele-distant.md).
- The exact name of each backend as you named it during configuration.

## How hybrid routing behaves

The hybrid router routes each agent reasoning step according to its estimated complexity:

1. Simple steps (information retrieval, formatting, direct tool calls) are handled by the default backend.
2. Complex steps (multi-hop reasoning, long synthesis, uncertain judgement) are escalated to the frontier backend.
3. When the cumulative cloud cost reaches `cost_ceiling_usd`, all remaining steps are handled by the default backend, whatever their complexity level.

The router does not guarantee a clean cut to the cent: steps already in flight when the ceiling is crossed finish on the backend that started them.

## Steps - Enable hybrid routing

`[llm.routing.hybrid]` is a subsection of `[llm.routing]`, which is itself
mandatory and has two required keys. Writing the hybrid table alone creates the
parent table without them, and the file then fails to load. Copy the whole
block, not the last three lines:

```toml
[llm.routing]
precise = "local-qwen3-8b"
fast    = "local-qwen3-4b"

[llm.routing.hybrid]
frontier          = "claude-anthropic"
cost_ceiling_usd  = 2.00
ceiling_action    = "stay_local"
```

The same file must already carry a `[llm]` section with a `default` key and at
least one `[[llm.backends]]` entry; both are required too.

- `precise` and `fast`: names of the backends used for deep reasoning and for lightweight extraction. Both are required, and both must match a declared backend.
- `frontier`: exact name of the cloud backend declared in `[llm.backends]`. The value cannot be empty.
- `cost_ceiling_usd`: ceiling in US dollars per routing session. Set a strictly positive value.
<!-- claim:hybrid-ceiling-action-decides-the-outcome -->
- `ceiling_action`: what happens when the ceiling is crossed. `stay_local`, the default, keeps the run going on the local backend, silently degraded. `hard_stop` ends the run cleanly with a structured error instead. Choose `hard_stop` when a silently local answer would be worse than no answer.

Restart the daemon after the change.

## Verification

The daemon does not validate this section at startup, and it prints nothing that
names your frontier or your ceiling. The one routing line it emits when it
starts carries the two backends of `[llm.routing]` and nothing else:

```
precise="local-qwen3-8b" fast="local-qwen3-4b" llm.routing.propagated
```

Hybrid routing shows up later, at the moment a step is escalated. Two lines tell
the whole story, and both carry the frontier name:

```
frontier="claude-anthropic" session_cost_usd=0.31 ceiling_usd=2 llm.hybrid.escalation.routed
frontier="claude-anthropic" reason="the session cost ceiling is reached" llm.hybrid.escalation.blocked
```

Run one deliberately complex task and look for either of them. To watch escalation and cumulative cost in real time, see the observability page. See [Monitor LLM costs](../observabilite/monitor-ai-costs.md).

## If it does not work

- **Nothing loads at all after the change:** the `[llm.routing]` table is incomplete. `precise` and `fast` are required as soon as the table exists, and writing `[llm.routing.hybrid]` alone is enough to create it.
- **No escalation ever happens, and no error anywhere:** a wrong `frontier` name is not caught when the daemon starts. It surfaces at the first escalation as `llm.hybrid.escalation.blocked` with `reason="the frontier backend is absent from the router"`. Check the exact name against **Settings - LLM Backends**, the match is case sensitive.
- **The ceiling is reached immediately:** the blocked line carries `reason="the session cost ceiling is reached"`. Raise `cost_ceiling_usd` or split your tasks into shorter sessions.
- **Every step stays local when you expect escalation:** the router only escalates on a step it judges complex. Test with an explicitly complex task before concluding the configuration is wrong.

> **Technical reference:** [Configuration](/reference/configuration) - the `[llm]` section, its backends and its routing keys.
