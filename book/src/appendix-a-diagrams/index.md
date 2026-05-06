# Diagrammes d'Architecture

> Sources PlantUML : [`docs/diagrams/`](https://github.com/nidal-z/apollia-os/tree/main/docs/diagrams)
> Régénérer : `just diagrams`

---

## C4 — Vues architecturales

## C4 — Vue Contexte
![C4 Context](c4-context-runtime.svg)

## C4 — Vue Container (16 crates)
![C4 Container](c4-container-runtime.svg)

## C4 — Composants Runtime Core (acteurs Tokio)
![C4 Component](c4-component-runtime-core.svg)

## Architecture — Application Desktop (Tauri v2 + Svelte 5)
![Desktop Architecture](component-desktop-architecture.svg)

## Architecture — Python SDK
![SDK Architecture](component-sdk-architecture.svg)

---

## Machines d'état

## Machine d'état — Agent (ProcessState)
![ProcessState](state-process.svg)

## Machine d'état — Tâche (TaskState)
![TaskState](state-task.svg)

## Machine d'état — Session de chat
![ChatSession State](state-chat-session.svg)

## Machine d'état — Circuit Breaker (ResilienceLayer)
![Circuit Breaker](state-circuit-breaker.svg)

---

## Séquences — Démarrage & arrêt

## Séquence — Démarrage Supervisor (16 phases)
![Supervisor Startup](seq-supervisor-startup.svg)

## Séquence — Configuration → Acteurs (démarrage ordonné)
![Config to Actors](seq-config-to-actors.svg)

## Séquence — Graceful Shutdown (SIGTERM → drain → exit)
![Graceful Shutdown](seq-graceful-shutdown.svg)

---

## Séquences — Exécution des tâches

## Séquence — Cycle de vie d'une tâche
![Task Lifecycle](seq-task-lifecycle.svg)

## Séquence — Boucle ORIA (Direct + Orchestré)
![ORIA Loop](seq-oria-loop.svg)

## Séquence — ORIA Mode Orchestré (ActorLoop)
![ORIA Orchestrated](seq-oria-orchestrated.svg)

## Séquence — Boucle ReAct run_tools
![ReAct run_tools](seq-run-tools-react.svg)

## Séquence — Bridge AIP (Rust ↔ Python via PyO3)
![AIP Bridge](seq-aip-bridge.svg)

## Séquence — HITL Flow complet (approve / reject)
![HITL Flow](seq-hitl-flow.svg)

---

## Séquences — Outils & intégrations

## Séquence — Appel outil natif
![Tool Call Native](seq-tool-call-native.svg)

## Séquence — Appel outil MCP
![Tool Call MCP](seq-tool-call-mcp.svg)

## Séquence — Cycle de vie session MCP (lazy start → handshake → call)
![MCP Session Lifecycle](seq-mcp-session-lifecycle.svg)

## Séquence — Appel LLM (ctx.llm.chat / complete / stream)
![LLM Call](seq-llm-call.svg)

## Séquence — Routing Multi-LLM (binding par agent)
![Multi-LLM Routing](seq-multi-llm-routing.svg)

---

## Séquences — Mémoire & observabilité

## Séquence — Mémoire (record + search FTS5)
![Memory Usage](seq-memory-usage.svg)

## Séquence — Timeline Aggregation (5 sources → chronologie unifiée)
![Timeline Aggregation](seq-timeline-aggregation.svg)

---

## Séquences — Chat & STT

## Séquence — Chat Libre (ReAct + streaming + mémoire)
![Chat Libre](seq-chat-libre.svg)

## Séquence — Injection User Memory dans le chat
![Chat User Memory](seq-chat-user-memory.svg)

## Séquence — Résumé de conversation (context window management)
![Conversation Summarize](seq-conversation-summarize.svg)

## Séquence — Speech-to-Text (hotkey → transcribe → clipboard)
![STT Flow](seq-stt-flow.svg)

## Séquence — Onboarding conversationnel (5 topics)
![Onboarding Flow](seq-onboarding-flow.svg)

---

## Séquences — Triggers & notifications

## Séquence — Trigger Fire (Cron / FileWatch / Webhook)
![Trigger Fire](seq-trigger-fire.svg)

## Séquence — CRUD Configuration opérationnelle (SQLite)
![Config CRUD](seq-config-crud.svg)

## Séquence — Dispatch des notifications (event → channel)
![Notification Dispatch](seq-notification-dispatch.svg)

---

## A2A Routing

## Séquence — Discovery + Invocation A2A (happy path)
![A2A Discovery Invoke](seq-a2a-discovery-invoke.svg)

## Séquence — Garde-fous A2A (max_hops, cycle_detected)
![A2A Guards](seq-a2a-guards.svg)

## Séquence — Chaîne A2A complète (A -> B -> C, happy path + CycleDetected)
![A2A Full Chain](seq-a2a-full-chain.svg)

## Séquence — Onboarding v2.1 complet (ADR-086)
![Onboarding v2.1](seq-onboarding-v2-detail.svg)

---

## Permissions & Sécurité

## Séquence — Moteur de permissions 3 couches (SafeList / PrefixRules / HITL)
![Permission Engine](seq-permission-engine.svg)

---

## Worker Agents *(32)*

## Séquence — Cycle de vie Worker Agent (manifest → SYSTEM_PROMPT → ReAct)
![Worker Agent Lifecycle](seq-worker-agent-lifecycle.svg)

## Séquence — Installation d'agents (bundled + communautaire)
![Agent Install](seq-agent-install.svg)
