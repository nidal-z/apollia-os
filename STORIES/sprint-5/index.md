# Sprint 5 — APIServer + CLI complete

**Sprint Goal :** Runtime operable sans modifier le code — `apollia-os start/stop/status/run` fonctionnels.
**Duree :** semaines 15-17
**Budget :** 38h estime / 32-40h budget

## Stories

| ID | Story | Crate | Taille | Statut |
|---|---|---|---|---|
| STORY-033 | APIServer axum Unix socket + TCP | apollia-runtime | L | 🔲 |
| STORY-034 | Routes REST tasks (POST/GET/DELETE) | apollia-runtime | M | 🔲 |
| STORY-035 | Routes REST agents (POST/GET/DELETE) | apollia-runtime | M | 🔲 |
| STORY-036 | SSE streaming pour taches | apollia-runtime | M | 🔲 |
| STORY-037 | CLI commandes niveau 1 (start/stop/status/run) | apollia-cli | L | 🔲 |
| STORY-038 | CLI commandes niveau 2 (agent/task/tools/memory/audit) | apollia-cli | L | 🔲 |
| STORY-039 | Supervisor demarrage ordonne + watchdog | apollia-runtime | L | 🔲 |
| STORY-040 | Graceful shutdown SIGTERM/drain 30s | apollia-runtime | M | 🔲 |

## Fichiers

- [Plan](plan.md)
- [STORY-033 — APIServer](story-033-apiserver-axum.md)
- [STORY-034 — Routes tasks](story-034-routes-rest-tasks.md)
- [STORY-035 — Routes agents](story-035-routes-rest-agents.md)
- [STORY-036 — SSE streaming](story-036-sse-streaming.md)
- [STORY-037 — CLI niveau 1](story-037-cli-niveau-1.md)
- [STORY-038 — CLI niveau 2](story-038-cli-niveau-2.md)
- [STORY-039 — Supervisor](story-039-supervisor.md)
- [STORY-040 — Graceful shutdown](story-040-graceful-shutdown.md)
