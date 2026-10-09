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
| **Verification checks** | The critic pass is wired on the orchestrated path. The manifest type carries a `check_commands` field for shell checks, and nothing runs it. The LLM critic is real; no deterministic shell check is. |
| **Speech-to-text** | Batch only. Transcribe and translate work; there is no streaming transcription. The feature is local-CPU and returns a service-unavailable response when the model is absent. |
| **Connectors** | Google is scoped to non-restricted, free-tier scopes (Gmail send and draft creation only, no restricted scopes; Calendar, Drive-file, and the sensitive document scopes). This is a deliberate free-tier posture, not a bug, but it bounds what an agent can do on Google. |
| **Token cost budget** | The step budget (steps, tool calls, wall-clock) is enforced, but the token-cost threshold is not: it defaults to effectively unlimited. Cost ceilings are not a hard stop. |
| **Runtime budget from config** | The ceiling of an agent run is fixed in the code, at 100 reasoning steps, 200 tool calls and 1200 seconds, and no key of `apollia.toml` changes it. A workflow run reads its own bounds, `[workflows] max_steps` and `wall_clock_secs`. |
| **MCP server** | Apollia as an MCP client is solid across three transports. The server side is a stdio bridge only, off by default, and it serves the agents and workflows the user exposed, not the native tools. |
| **Model recommendation** | Onboarding ranks models for the local machine, but three of its inputs are judgement rather than measurement: the capability scores in the curated generation table (`families.toml`), the capability each quantization retains, and the memory bandwidth figures behind the speed term. Memory costs, compatibility and layer placement are computed from the files themselves. The catalogue reads a signed online index and falls back to the index shipped in the binary, so a machine with no network is offered the cards its build knew, which are as old as that build. Split GGUF files are not offered, because the downloader fetches one URL. |
| **Triggers** | Cron, interval, one-shot, file-watch and webhook sources are wired. There is no email or Slack trigger. |
| **Copilot / meta-LLM layer** | The "more transparent than a cloud assistant" ambition is roughly a third wired. Of the meta commands, only Next Steps is a live LLM call end to end; the coach engine is real and the connector wizard reaches it at its coaching step, and the rest are heuristics or templates. The contracts are in place; the secondary LLM is largely still to connect. |
| **Streaming usage** | Token streaming is real, but the stream's done event carries no usage figures. |

## Missing or dead code

| Area | Reality |
|---|---|
| **Sharded GGUF loading** | Missing. The embedded `llama-server` engine loads a single-file GGUF model per server process. A code comment suggesting sharded loading does not reflect a wired path. |
| **Actor restart** | None, by decision. An actor that dies is not restarted: the runtime reports the loss and degrades the capability that actor served (see [supervision](/architecture/decisions#supervision)). The inference engines and the speech-to-text runner are separate processes, and the daemon does respawn those. |
| **Direct execute via the unified path** | The `execute()` direct-via-unified path is a stub. The real direct path runs through a separate entry point; the stub is secondary. |

## Documentation drift this cartography corrects

Older subsystem notes overclaimed in specific ways. For the record, and so no
reader trusts the stale version:

- The SDK contract is `sdk/apollia/types.py`, not an `sdk/apollia/stubs/`
  directory (which does not exist).
- The runtime context `ctx` is seventeen services, not the earlier flat shape.
  See the [SDK reference](/reference/sdk).
- Sharded GGUF loading and an actor restart policy were described as working;
  neither exists, as stated above.
- Replay was described as running a run again; what exists steps through the
  captured trace and runs nothing again.

## What this means for an adopter

The moat is real and demonstrable: bounded autonomy with an enforced step
budget, a signed and verifiable audit trail, permissions with
human oversight and autonomy tiers, a guard that refuses chained or redirected
shell commands, and a governed tool path that native and MCP tools both pass
through. Prompt-injection detection is **not** part of that list, and no
component of the tree provides it: what the guard above screens is shell
injection. The debt is
mostly at the edges: the shell-check
half of verification, cost ceilings, and the copilot layer.
Knowing exactly where those edges are is the point of this page.
