---
title: Configure hybrid local + frontier routing
slug: /operator-help/installation/configure-hybrid-routing
sidebar_position: 7
---

# Configure hybrid local + frontier routing

> For any operator who wants to combine a fast local model for simple steps and a powerful cloud model for complex steps, with an automatic cost ceiling.

## Prerequisites

- At least one local backend declared in `apollia.toml` (`.gguf` file or Ollama server). See [Download local models](telecharger-des-modeles-locaux.md).
- At least one cloud (frontier) backend declared. See [Connect a remote model](connecter-un-modele-distant.md).
- The exact name of each backend as you named it during configuration.

## How hybrid routing behaves

The hybrid router routes each agent reasoning step according to its estimated complexity:

1. Simple steps (information retrieval, formatting, direct tool calls) are handled by the local model.
2. Complex steps (multi-hop reasoning, long synthesis, uncertain judgement) are escalated to the frontier backend.
3. When the cumulative cloud cost reaches `cost_ceiling_usd`, all remaining steps are handled locally, whatever their complexity level.

The router does not guarantee a clean cut to the cent: steps already in flight when the ceiling is crossed finish on the backend that started them.

## Steps - Enable hybrid routing

Edit `apollia.toml` and add the following section:

```toml
[llm.routing.hybrid]
frontier          = "claude-anthropic"
cost_ceiling_usd  = 2.00
ceiling_action    = "stay_local"
```

- `frontier`: exact name of the cloud backend declared in `[llm.backends]`. The value cannot be empty.
- `cost_ceiling_usd`: ceiling in US dollars per routing session (must be strictly positive). Any zero or negative value is rejected at startup.
<!-- claim:hybrid-ceiling-action-decides-the-outcome -->
- `ceiling_action`: what happens when the ceiling is crossed. `stay_local`, the default, keeps the run going on the local backend, silently degraded. `hard_stop` ends the run cleanly with a structured error instead. Choose `hard_stop` when a silently local answer would be worse than no answer.

Restart the daemon after the change.

## Verification

Invalid values are detected when the daemon starts. If the config is correct, the logs show:

```
llm.routing=hybrid frontier=claude-anthropic ceiling_usd=2.00 "routing.activated"
```

To watch escalation and cumulative cost in real time, see the observability page. See [Monitor LLM costs](../observabilite/surveiller-les-couts-llm.md).

## If it does not work

- **"unknown frontier backend" at startup:** the `frontier` value does not match any backend declared in `[llm.backends]`. Check the exact name, the match is case sensitive.
- **"invalid ceiling" at startup:** `cost_ceiling_usd` is zero, negative or missing. Set a strictly positive value (example: `0.50`).
- **The ceiling is reached immediately:** your ceiling is too low for the tasks you are running. Raise `cost_ceiling_usd` or split your tasks into shorter sessions.
- **Every step stays local when you expect escalation:** your local model may be judged good enough for your tasks. Lower `cost_ceiling_usd` or test with an explicitly complex task to confirm the router works.

> **Technical reference:** [Apollia reference](/reference) - multi-backend routing parameters, fallback policy, per-step cost computation.
