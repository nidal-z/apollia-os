# ADR-034 — Chat hybride : sessions, streaming, HITL inline

**Date :** 2026-03-20
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 18

---

## Contexte

Les agents Apollia OS sont aujourd'hui exclusivement **programmés** : triggers, pipelines, exécution en arrière-plan (fire-and-forget via TaskRouter). Il manque un mode **interactif** où l'utilisateur ouvre une conversation libre, pose des questions, et l'agent exécute à la demande avec retour en temps réel.

Deux besoins distincts émergent :
1. **Chat Libre** : utiliser le LLM directement avec des outils natifs (bash, file_io) sans agent Python — pour l'exploration, l'aide rapide, les tâches ad-hoc.
2. **Chat Agent** : converser avec un agent Python installé, qui a accès à sa mémoire et ses outils spécialisés — pour l'interaction approfondie avec un agent métier.

Le `TaskRouter` existant est conçu pour des tâches fire-and-forget, stateless entre elles, avec un semaphore de concurrence par agent. Le chat a des sémantiques fondamentalement différentes : sessions longues, état mutable (historique, outils autorisés), streaming token-by-token, HITL inline avec escalade progressive (Accept → Always Accept).

Forcer le chat dans le moule du `TaskRouter` créerait des abstractions qui fuient.

**Contraintes :**
- Principe #1 (Local-first) : toute la persistance est locale (SQLite)
- Principe #5 (Un acteur, une responsabilité) : pas de surcharge du TaskRouter
- Principe #7 (Garde-fous non-négociables) : StepBudget appliqué par échange
- L'infrastructure existante (EventBus, SSE, Tauri event bridge) doit être réutilisée

---

## Décision

Nous introduisons un **chemin d'exécution séparé** pour le chat, avec un nouvel acteur `ChatSessionManager` (position 13 dans le Supervisor) et deux modes dans une même interface :

| Aspect | Décision |
|---|---|
| **Deux modes** | Chat Libre (BuiltInChatAgent Rust) + Chat Agent (agent Python installé) |
| **Concurrence** | `ChatSessionManager` indépendant du `TaskRouter` — pas de contention sur les semaphores |
| **Communication** | POST + SSE (pas de WebSocket) |
| **Streaming** | Token-by-token via `LlmRouter.stream()` + `ChatToken` RuntimeEvent (Chat Libre) ; progress events (Chat Agent) |
| **HITL** | Tous les outils requièrent approbation HITL par défaut en mode chat ; 3 boutons : Accept / Refuse / Always Accept (whitelist per-session) |
| **Persistance** | `chat.db` SQLite séparé (sessions, messages, autorisations outils) |
| **Mémoire** | Historique persistant dans `chat.db` ; alimentation mémoire épisodique à l'initiative de l'agent (Principe #6) |

### Chat Libre — BuiltInChatAgent

Pour le Chat Libre, le runtime embarque un agent Rust natif (`BuiltInChatAgent`) qui implémente une boucle ReAct sans Python :

```
User message
  → CompletionRequest (system_prompt + history + tool_specs)
  → LlmRouter.stream(request)
  → Parse response :
      Texte → émettre ChatToken par token → sauver → fin
      Tool call → vérifier autorisation session :
          Autorisé → exécuter → feed result → boucle
          Non autorisé → ChatApprovalRequired → await décision
            Accept → exécuter → boucle
            AlwaysAccept → whitelist + exécuter → boucle
            Refuse → "outil refusé" → boucle
  → StepBudget guard (max iterations par échange)
```

### Chat Agent — AIPBridge direct

Pour le Chat Agent, `ChatSessionManager` appelle `AIPBridge.call_run()` directement (pas via TaskRouter), en convertissant la session en `AIPTask` avec `task.history`.

### Séparation du TaskRouter

| Aspect | Tasks (existant) | Chat (nouveau) |
|---|---|---|
| Point d'entrée | `POST /api/v1/tasks` → TaskRouter | `POST /api/v1/sessions/:id/messages` → ChatSessionManager |
| Concurrence | Semaphore per-agent | Un échange actif par session |
| Lifecycle | Fire-and-forget | Session longue, multiples échanges |
| État | Stateless entre tasks | Stateful (historique, outils autorisés) |
| Events | TaskStarted/Completed/Failed | ChatMessageSent/ResponseCompleted/Token |
| Observabilité | tasks.db | chat.db |

---

## Alternatives considérées

### Option A — WebSocket pour le chat (rejetée)

**Pour :** Bidirectionnel natif, pas besoin de SSE séparé, standard pour le chat temps réel.
**Contre :** Aucune infrastructure WebSocket existante dans le projet. Apollia utilise axum + SSE partout (tasks, dashboard, triggers). Ajouter WebSocket introduirait une deuxième pile de communication (upgrade HTTP, frame parsing, reconnection). POST + SSE couvre le besoin (le client envoie via POST, reçoit via SSE persistant).

### Option B — Session = single long-running task dans le TaskRouter (rejetée)

**Pour :** Réutilise l'infra existante à 100%, pas de nouveau module.
**Contre :** Incompatible avec le modèle stateless du TaskRouter. Une session de chat est stateful (historique accumulé, outils autorisés). Le TaskRouter drain les tasks au shutdown — une session de chat ne doit pas être drainée. Le semaphore de concurrence agent bloquerait les tasks background pendant un chat. Le streaming token-by-token n'a pas d'équivalent dans le modèle task (SSE task se ferme à TaskCompleted).

### Option C — Chat via le TaskRouter avec extensions (rejetée)

**Pour :** Un seul chemin d'exécution à maintenir.
**Contre :** Nécessiterait d'ajouter au TaskRouter : état par session, streaming continu, HITL inline avec AlwaysAccept, bypass du semaphore pour le chat. Ces extensions dénatureraient le TaskRouter (Principe #5 violé). Le TaskRouter est conçu pour fire-and-forget — le forcer à gérer des sessions longues créerait des abstractions qui fuient.

### Option retenue — Chemin d'exécution séparé (ChatSessionManager)

**Pour :** Séparation claire des responsabilités (Principe #5). Le TaskRouter reste simple et stateless. Le chat a ses propres sémantiques, sa propre DB, ses propres events. Pas de compromis architectural — chaque système fait ce pour quoi il est conçu.
**Compromis acceptés :** Duplication partielle de code (EventBus subscriber, SSE stream setup). Deux acteurs à maintenir au lieu d'un. 12 nouveaux RuntimeEvent variants dans `apollia-core`.

---

## Conséquences

**Positives :**
- Le chat est un citoyen de première classe avec ses propres sémantiques, pas un hack sur le TaskRouter
- Le streaming token-by-token fonctionne nativement (pas de contorsion pour faire rentrer le streaming dans le modèle task)
- Le HITL inline avec AlwaysAccept est propre (whitelist per-session, pas de contamination du manifest agent)
- Le TaskRouter existant est inchangé — zéro risque de régression sur les tasks background
- Le BuiltInChatAgent en Rust permet un Chat Libre sans Python (démarrage instantané)

**Négatives / Compromis :**
- 12 nouveaux RuntimeEvent variants (`Chat*`) alourdissent l'enum dans `apollia-core`
- `chat.db` est une base SQLite supplémentaire à gérer (backup, migrations)
- Le `ChatSessionManager` duplique certains patterns du TaskRouter (EventBus subscribe, SSE setup)
- Le Chat Agent peut potentiellement accéder à un agent Python en parallèle avec une task background — la concurrence dépend du thread-safety de l'agent

**Neutres / À surveiller :**
- Le `ChatToken` RuntimeEvent est émis à très haute fréquence (un par token LLM) — l'EventBus broadcast (buffer 1024) devrait supporter, mais à monitorer sous charge
- Le Tauri event bridge traite `ChatToken` via un event séparé `"chat-token"` (pas de refresh IPC complet) — pattern nouveau à valider en Sprint 18
- La concurrence Chat vs Tasks pour un même agent Python non thread-safe pourrait nécessiter un lock ou une instance AIPBridge distincte — à décider pendant STORY-202

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : Renforcé. `chat.db` est local, l'historique ne quitte jamais la machine.
- **Principe #5 — Un acteur, une responsabilité** : Respecté. Le `ChatSessionManager` est un acteur dédié (position 13 Supervisor), le `TaskRouter` n'est pas modifié.
- **Principe #6 — Mémoire à initiative de l'agent** : Respecté. En Chat Agent, l'agent accède à `ctx.memory` librement. En Chat Libre, pas d'accès mémoire (pas de namespace).
- **Principe #7 — Garde-fous non-négociables** : Renforcé. Chaque échange consomme un `StepBudget` frais. Tous les outils passent par HITL en mode chat (plus restrictif que le mode background).
- **Principe #8 — CLI humaine, API machine** : Respecté. 7 endpoints REST (`/api/v1/sessions/*`), SSE stream, Tauri IPC commands.

---

## Liens

- Stories associées : STORY-198 → STORY-209 (Sprint 18 complet)
- ADR précédent sur HITL : ADR-023 (HITL is_resumed + input_response + tools_requiring_approval)
- ADR précédent sur le Supervisor : ADR-027 (processus unique Tauri + runtime embarqué)
- Spec complète : `docs/specs/sprint-18-spec.md`
