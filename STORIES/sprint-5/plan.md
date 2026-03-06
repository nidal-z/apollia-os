# Sprint 5 — Plan

**Sprint Goal :** Runtime operable sans modifier le code — `apollia-os start` demarre le runtime, `apollia-os run hello-agent "Bonjour"` execute un agent, `apollia-os status` affiche l'etat, `apollia-os stop` arrete proprement.
**Duree estimee :** 38h / budget 32-40h (3 semaines)
**Dates :** semaines 15-17

---

## Stories du sprint (ordre d'implementation)

| Priorite | ID | Story | Crate | Taille | Estime | Depend de |
|---|---|---|---|---|---|---|
| 1 | STORY-033 | APIServer axum Unix socket + TCP | apollia-runtime | L | 6h | Sprint 4 ✅ |
| 2 | STORY-034 | Routes REST tasks (POST/GET/DELETE) | apollia-runtime | M | 3h | STORY-033 |
| 3 | STORY-035 | Routes REST agents (POST/GET/DELETE) | apollia-runtime | M | 3h | STORY-033 |
| 4 | STORY-036 | SSE streaming pour taches | apollia-runtime | M | 3h | STORY-034 |
| 5 | STORY-039 | Supervisor demarrage ordonne + watchdog | apollia-runtime | L | 6h | STORY-033, 034, 035 |
| 6 | STORY-040 | Graceful shutdown SIGTERM/drain 30s | apollia-runtime | M | 3h | STORY-039 |
| 7 | STORY-037 | CLI commandes niveau 1 (start/stop/status/run) | apollia-cli | L | 8h | STORY-039, 040 |
| 8 | STORY-038 | CLI commandes niveau 2 (agent/task/tools/memory/audit) | apollia-cli | L | 6h | STORY-037 |

**Jalons intermediaires :**
- Apres STORY-035 (semaine 15-16) : "APIServer demarre, routes REST fonctionnelles avec curl" — premier jalon demo-able
- Apres STORY-040 (semaine 16) : "Supervisor orchestre startup/shutdown complet" — runtime autonome
- Apres STORY-037 (semaine 17) : Sprint Goal atteint — `start/stop/status/run` fonctionnels

**Note :** STORY-034 et STORY-035 sont parallelisables car elles dependent uniquement de STORY-033 (skeleton APIServer). STORY-036 (SSE) depend des routes tasks (STORY-034) pour avoir les endpoints a streamer.

---

## Dependances verifiees

| Dependance | Statut | Story dependante |
|---|---|---|
| `axum` 0.7 dans workspace deps | ✅ | STORY-033 |
| `tower` 0.4 + `tower-http` 0.5 dans workspace deps | ✅ | STORY-033 |
| `clap` v4 derive dans workspace deps | ✅ | STORY-037, 038 |
| `EventBus` broadcast (STORY-006) | ✅ Sprint 1 | STORY-033, 036, 039, 040 |
| `AgentRegistryHandle` (STORY-008) | ✅ Sprint 1 | STORY-035, 039 |
| `ToolRegistryHandle` (STORY-011) | ✅ Sprint 2 | STORY-035, 038 |
| `TaskRouterHandle` (STORY-032) | ✅ Sprint 4 | STORY-034, 039 |
| `ExecutionCoordinator` (STORY-031) | ✅ Sprint 4 | STORY-039 |
| `MemoryManager` (STORY-021) | ✅ Sprint 3 | STORY-038 |
| `AuditTrailHandle` (STORY-016) | ✅ Sprint 2 | STORY-038 |
| CLI memory command existante (STORY-023) | ✅ Sprint 3 | STORY-038 |

---

## Risques identifies

### Risque #1 — Premiere utilisation d'axum : Unix socket + TCP dual listener (ELEVE)
- **Contexte :** axum 0.7 supporte TCP nativement, mais le dual binding Unix socket + TCP necessite `hyper` + `tower` pour le listener Unix. C'est la premiere utilisation d'axum dans le projet.
- **Impact :** STORY-033 pourrait prendre plus de 6h si le binding Unix socket est complexe.
- **Mitigation :** Commencer par TCP seul, ajouter Unix socket ensuite. `tokio::net::UnixListener` + `hyper::server::conn` permettent le binding custom. Prevoir 2h de buffer.

### Risque #2 — SSE streaming via EventBus (MOYEN)
- **Contexte :** Le SSE necessite de transformer les `RuntimeEvent` du broadcast channel en `text/event-stream`. axum supporte SSE via `axum::response::sse::Sse`, mais le mapping EventBus → SSE par task_id necessite un filtrage.
- **Impact :** STORY-036 pourrait necessiter un composant intermediaire de filtrage.
- **Mitigation :** Utiliser `EventBusReceiver` avec filtrage cote serveur. Un `BroadcastStream` tokio-stream peut alimenter le SSE.

### Risque #3 — Supervisor orchestration de 6 acteurs avec timeout (ELEVE)
- **Contexte :** Le Supervisor doit demarrer 6 acteurs en sequence, chacun emettant `RuntimeEvent::Ready`. Un timeout de 10s par acteur et un ordre strict sont requis. C'est le composant le plus complexe du sprint.
- **Impact :** STORY-039 est critique pour le Sprint Goal.
- **Mitigation :** Pattern simple : boucle sequentielle avec `tokio::time::timeout`. Pas de state machine complexe. Chaque acteur retourne son handle apres spawn, le Supervisor attend le Ready via EventBus.

### Risque #4 — Integration CLI ↔ APIServer via Unix socket (MOYEN)
- **Contexte :** La CLI doit communiquer avec le runtime via Unix socket (`/tmp/apollia.sock`). `reqwest` supporte les Unix sockets mais c'est la premiere utilisation de ce pattern.
- **Impact :** STORY-037 depend d'un client HTTP Unix socket fonctionnel.
- **Mitigation :** Ajouter `hyper-util` ou utiliser `tokio::net::UnixStream` directement. Evaluer si `reqwest` avec feature `unix-socket` suffit ou si un client plus leger est preferable.

---

## Crates impactees

| Crate | Stories | Etat avant sprint | Etat apres sprint |
|---|---|---|---|
| `apollia-runtime` | STORY-033, 034, 035, 036, 039, 040 | EventBus + Registry + Router + Coordinator | + APIServer (axum) + Supervisor + Graceful shutdown |
| `apollia-cli` | STORY-037, 038 | Skeleton (memory command only) | CLI complete : start/stop/status/run + agent/task/tools/memory/audit |
| `apollia-core` | (aucune nouvelle story) | Stable | Potentiellement nouveaux RuntimeEvent (ShutdownRequested, AllReady) |

---

## Nouvelles dependances potentielles

| Crate | Usage | Decision |
|---|---|---|
| `tokio-stream` 0.1 (feature `sync`) | Conversion broadcast → Stream pour SSE | ✅ Ajout workspace dans STORY-036 |
| `reqwest` (feature unix-socket) | Client HTTP CLI → APIServer | A evaluer dans STORY-037. Alternative : client leger custom |
| `hyper-util` | Unix socket listener pour axum | A evaluer dans STORY-033. Necessaire si `axum::serve` ne supporte pas UnixListener directement |

---

## Definition of Done du sprint

- [ ] Sprint Goal atteint : `apollia-os start` → `apollia-os run hello-agent "Bonjour"` → `apollia-os status` → `apollia-os stop` fonctionnels
- [ ] `cargo test --workspace` passe (0 test echoue)
- [ ] `cargo clippy --workspace -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] `sprint-index.md` mis a jour (toutes les stories ✅)
- [ ] `sprint-5/bilan.md` redige
- [ ] Au moins 1 ADR cree si deviation architecturale

---

## Ordre d'implementation detail

```
semaine 15
  jour 1-3 : STORY-033 — APIServer axum Unix socket + TCP (6h)
             PoC: axum bind TCP 7771 + Unix socket /tmp/apollia.sock
             Impl: APIServer struct + start() + health endpoint
  jour 4   : STORY-034 — Routes REST tasks (3h)
             POST/GET/DELETE /api/v1/tasks + TaskRouterHandle integration
  jour 5   : STORY-035 — Routes REST agents (3h)
             POST/GET/DELETE /api/v1/agents + AgentRegistryHandle integration

semaine 16
  jour 1   : STORY-036 — SSE streaming pour taches (3h)
             GET /api/v1/tasks/{id}/stream + EventBus → SSE
  jour 2-4 : STORY-039 — Supervisor demarrage ordonne + watchdog (6h)
             Startup sequentiel 6 acteurs + RestartPolicy + timeout
  jour 5   : STORY-040 — Graceful shutdown SIGTERM/drain 30s (3h)
             Signal handler + ShutdownRequested broadcast + drain

semaine 17
  jour 1-3 : STORY-037 — CLI commandes niveau 1 (8h)
             start/stop/status/run + client Unix socket + --json
  jour 4-5 : STORY-038 — CLI commandes niveau 2 (6h)
             agent/task/tools/memory/audit + integration routes REST
  buffer   : Bilan sprint + dette technique
```
