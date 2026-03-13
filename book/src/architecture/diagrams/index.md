# Diagrammes d'Architecture

> Sources PlantUML : [`docs/diagrams/`](https://github.com/nidal-z/apollia-os/tree/main/docs/diagrams)
> Régénérer : `just diagrams`

## C4 — Vue Contexte
![C4 Context](c4-context-runtime.svg)

## C4 — Vue Container (9 crates)
![C4 Container](c4-container-runtime.svg)

## C4 — Composants Runtime Core
![C4 Component](c4-component-runtime-core.svg)

## Machine d'état — Agent (ProcessState)
![ProcessState](state-process.svg)

## Machine d'état — Tâche (TaskState)
![TaskState](state-task.svg)

## Séquence — Cycle de vie d'une tâche
![Task Lifecycle](seq-task-lifecycle.svg)

## Séquence — Boucle ORIA
![ORIA Loop](seq-oria-loop.svg)

## Séquence — Appel outil natif
![Tool Call Native](seq-tool-call-native.svg)

## Séquence — Appel outil MCP
![Tool Call MCP](seq-tool-call-mcp.svg)

## Séquence — Mémoire
![Memory Usage](seq-memory-usage.svg)

## Séquence — Appel LLM (ctx.llm.chat / complete / stream)
![LLM Call](seq-llm-call.svg)

## Séquence — Boucle ReAct run_tools()
![ReAct run_tools](seq-run-tools-react.svg)

## Séquence — Démarrage Supervisor (7 phases)
![Supervisor Startup](seq-supervisor-startup.svg)

## Séquence — Trigger Fire (Cron / FileWatch / Webhook)
![Trigger Fire](seq-trigger-fire.svg)

## Séquence — HITL Flow complet (approve / reject)
![HITL Flow](seq-hitl-flow.svg)

## Séquence — ORIA Mode Orchestré (ActorLoop)
![ORIA Orchestrated](seq-oria-orchestrated.svg)

---

## Pipelines multi-agents *(Sprint 12)*

## Topologie — Pipeline (fan-out / fan-in / fallback / condition)
![Pipeline Topology](component-pipeline-topology.svg)

## Machine d'état — PipelineRun & StepRun
![Pipeline State](state-pipeline.svg)

## Séquence — Exécution d'un pipeline (layers topologiques)
![Pipeline Execution](seq-pipeline-execution.svg)

## Séquence — HITL dans un pipeline (suspend / resume)
![Pipeline HITL](seq-pipeline-hitl.svg)

---

## Observabilité *(Sprint 13)*

## Séquence — Timeline Aggregation (5 sources → chronologie unifiée)
![Timeline Aggregation](seq-timeline-aggregation.svg)
