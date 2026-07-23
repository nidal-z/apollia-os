---
sidebar_position: 9
title: 9. Risks and technical debt
---

# 9. Risks and technical debt

This page states, plainly, what is partial and what is absent. It is derived
from a code-certified capability review, not from design intent. A cartography
that hides its gaps is not credible, so the gaps are here in full. Statuses use
three words: **partial** (wired but incomplete), **missing** (not wired, a stub,
or dead code), and, where relevant, a note that a comment in the code overstates
reality.

## Partial: wired, but with a stated limit

| Area | The honest limit |
|---|---|
| **Verification checks** | The critic pass is wired on the orchestrated path, but running an agent's declared shell `check_commands` under governance is not; that invoker is a no-op today. The LLM critic is real; the deterministic shell checks are a follow-up. |
| **Speech-to-text** | Batch only. Transcribe and translate work; there is no streaming transcription. The feature is local-CPU and returns a service-unavailable response when the model is absent. |
| **Connectors** | Google is scoped to non-restricted, free-tier scopes (Gmail send and draft creation only, no restricted scopes; Calendar, Drive-file, and the sensitive document scopes). This is a deliberate free-tier posture, not a bug, but it bounds what an agent can do on Google. |
| **Desktop orchestrated path** | The orchestrated execution path is a no-op in the desktop app; the direct path is bounded and wired. |
| **Token cost budget** | The step budget (steps, tool calls, wall-clock) is enforced, but the token-cost threshold is not: it defaults to effectively unlimited. Cost ceilings are not yet a hard stop. |
| **Runtime budget from config** | The step ceiling is enforced with a safe default, but reading that ceiling from `apollia.toml` at runtime is still to wire. |
| **Inbound MCP server** | Apollia as an MCP client is solid across three transports. The inbound MCP server (Apollia exposing itself) is partial: stdio only. |
| **Triggers** | Cron, interval, one-shot, and file-watch sources are wired. The webhook trigger source is a no-op stub. There is no email or Slack trigger. |
| **Copilot / meta-LLM layer** | The "more transparent than a cloud assistant" ambition is roughly a third wired. Of the meta commands, only Next Steps is a live LLM call end to end; the coach engine is real but has no UI wired to it, and the rest are heuristics or templates. The contracts are in place; the secondary LLM is largely still to connect. |
| **Streaming usage** | Token streaming is real, but the stream's done event carries no usage figures. |

## Missing or dead code

| Area | Reality |
|---|---|
| **Sharded GGUF loading** | Missing. The embedded `llama-server` engine loads a single-file GGUF model per server process. A code comment suggesting sharded loading does not reflect a wired path. |
| **Embeddings** | Missing. The embeddings path is a stub, not a delivered capability. |
| **Inference health monitor and auto-restart** | Missing. The daemon spawns and load-locks the embedded `llama-server` inference process, but there is no health monitoring or automatic restart. A code comment claiming otherwise is wrong. |
| **Actor restart policy** | The actor supervisor defines a restart policy, but it is not enforced. It is effectively dead code today. |
| **Direct execute via the unified path** | The `execute()` direct-via-unified path is a stub. The real direct path runs through a separate entry point; the stub is secondary. |

## Documentation drift this cartography corrects

Older subsystem notes overclaimed in specific ways. For the record, and so no
reader trusts the stale version:

- The SDK contract is `sdk/apollia/types.py`, not an `sdk/apollia/stubs/`
  directory (which does not exist).
- The runtime context `ctx` is fifteen services, not the earlier flat shape.
  See the [SDK reference](/reference/sdk).
- Sharded GGUF, runner auto-restart, and the actor restart policy were described
  as working; they are not, as stated above.
- Replay was described as a fidelity feature; it was abandoned by decision.

## What this means for an adopter

The moat is real and demonstrable: bounded autonomy with an enforced step
budget, a signed and verifiable audit trail with rollback, permissions with
human oversight and autonomy tiers, structural injection detection, and a
governed tool path that native and MCP tools both pass through. The debt is
mostly at the edges: hardening the inference sidecar, closing the shell-check
half of verification, wiring cost ceilings, and finishing the copilot layer.
Knowing exactly where those edges are is the point of this page.
