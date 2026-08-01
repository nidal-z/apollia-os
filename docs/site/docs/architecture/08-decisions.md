---
sidebar_position: 8
title: 8. Architecture decisions
---

# 8. Architecture decisions

Structural choices are captured as numbered architecture decision records. This
page synthesizes the ones that shape the system most; each is cited by its bare
identifier. The full records live in the project's decision log.

## The most structuring decisions

- **ADR-001, foundations and stack.** Rust plus Tokio for the runtime, PyO3 for
  the Python bridge, `llama.cpp` for local inference, SQLite for persistence.
  This fixes the sovereignty and zero-dependency posture at the base.
- **ADR-002, PyO3 bridge and trait decoupling.** Agents are Python behind a
  bridge that exposes services through Rust traits, so the agent contract is
  decoupled from the implementation and mockable.
- **ADR-005, ORIA execution model.** The autonomous engine: a ReAct loop in
  direct and orchestrated modes, on the Tokio actor core.
- **ADR-007, inference as a multi-runner sidecar.** Local inference runs in a
  separate supervised runner process, isolating model crashes from the daemon.
- **ADR-015, permission and tool governance.** The permission engine, scopes,
  and the governed tool path that every tool call passes through.
- **ADR-037, host driving contract.** A generated, versioned OpenAPI surface
  plus TypeScript and Python host SDKs, so a host product drives the runtime
  without reverse-engineering it. This is the integration product the beachhead
  needed.
- **ADR-038, orchestrated step arguments.** A hybrid contract: the reasoner
  fills structured step arguments in GBNF at plan time, with just-in-time
  extraction as a fallback at execution. This is what lets the orchestrated path
  drive real native tools with structured arguments.
- **ADR-039, verification and critic on the orchestrated path.** A completed
  orchestrated run is verified by a critic, the verdict is audited as a runtime
  event, and a failing verdict triggers bounded re-planning under the shared
  budget, gated by autonomy tier.

## A decision to not build

- **Replay was abandoned (2026-07-08).** Re-executing and comparing a run was
  judged to carry no functional or regulatory value for its cost.
  Accountability rests on the signed journal and verification, not on replay. It is recorded here so its absence reads as a choice, not a gap. The
  related plan-construction audit is ADR-033.

## Supporting decisions

The CLI taxonomy and AI-native surface (ADR-034, ADR-035, ADR-036), the memory
and context architecture (ADR-010), human-in-the-loop (ADR-013), secrets and API
auth (ADR-016), the MCP client and transports (ADR-017, ADR-018), connectors
(ADR-019), the desktop app (ADR-020), the SDK and A2A routing (ADR-023, ADR-024,
ADR-025), and the unified plan model and chat-native plan engine (ADR-031,
ADR-032) round out the record.
