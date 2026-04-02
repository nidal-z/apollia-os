# Annexe B — Glossaire

Définitions des termes techniques utilisés dans ce livre.

---

## A

**Acteur Tokio** — Pattern de concurrence utilisé par le runtime : chaque composant (EventBus, AgentRegistry, TaskRouter…) est une tâche asynchrone qui possède exclusivement son état et communique avec les autres par messages via un canal `mpsc`. Aucun état partagé entre acteurs.

**AIP** (Agent Interface Protocol) — Le contrat minimal entre un agent Python et le runtime Rust. Deux méthodes suffisent : `manifest()` (qui décrit l'agent) et `async def run(task, ctx)` (qui exécute une tâche). Basé sur le duck typing Python — pas de classe de base obligatoire.

**ActorLoop** — Le composant d'ORIA qui exécute un plan step par step en mode orchestré. Il appelle les outils, gère les erreurs, et applique le StepBudget à chaque étape.

## C

**Circuit breaker** — Mécanisme de protection par outil. Après 3 échecs consécutifs (erreurs transitoires), le circuit "s'ouvre" et rejette immédiatement les appels pendant 30 secondes (cooldown). Une tentative unique est ensuite autorisée (half-open) : succès = circuit fermé, échec = nouveau cooldown. Implémenté dans `ResilienceLayer`.

## D

**Duck typing** — Convention Python où le runtime vérifie la présence de méthodes (`manifest()`, `run()`) plutôt qu'un héritage de classe. Votre agent n'a pas besoin d'hériter d'une classe de base — il suffit d'implémenter les bonnes méthodes.

## E

**EventBus** — Acteur central qui diffuse les événements système (`RuntimeEvent`) à tous les abonnés via un canal `broadcast`. Exemples : `TaskStarted`, `TaskCompleted`, `ToolCircuitBroken`, `ShutdownRequested`.

## F

**FTS5** — Module SQLite de recherche full-text, version 5. Utilisé par le Memory Engine pour la recherche dans les souvenirs de l'agent. Tokenizer : `unicode61` (adapté au français et à l'accentuation).

## H

**HITL** (Human-In-The-Loop) — Mécanisme de validation humaine. Quand un agent appelle un outil marqué dans `tools_requiring_approval`, la tâche passe en état `input_required` et attend qu'un humain approuve ou rejette l'action via CLI (`apollia-os task resume`), API REST, ou Desktop.

## M

**mpsc** — "Multiple Producer, Single Consumer" — type de canal Tokio utilisé pour la communication entre acteurs. Plusieurs acteurs peuvent envoyer des messages, un seul les reçoit et les traite séquentiellement.

## N

**Namespace** — (1) En mémoire : espace de stockage isolé par agent. Chaque agent a son `memory_namespace` privé et peut accéder à des `shared_memory_namespaces`. (2) En Linux : isolation de processus via `unshare` (user namespace, mount namespace, network namespace).

## O

**ORIA** (Observer-Reasoner-Actor) — Le moteur d'exécution du runtime. En mode direct, il supervise le `run()` de l'agent et applique les garde-fous. En mode orchestré, le Reasoner génère un plan et l'ActorLoop l'exécute.

## P

**Pipeline** — Orchestration déclarative de plusieurs agents en séquence ou parallèle (DAG). Supporte fan-out, fan-in, conditions, fallback, et HITL intégré. Défini en JSON via API et persisté en SQLite.

**ProcessState** — Machine d'état d'un agent : `Initializing` → `Active` → `Stopping` → `Stopped`. Un agent peut aussi être `Degraded` (outil optionnel manquant, LLM indisponible).

## R

**Reasoner** — Composant d'ORIA qui, en mode orchestré, utilise un appel LLM pour transformer le `system_prompt` de l'agent et l'input de la tâche en un `ExecutionPlan` JSON (liste de steps avec outils et paramètres).

**ResilienceLayer** — Couche de protection qui enveloppe chaque appel d'outil avec un circuit breaker et une politique de retry. Appliquée automatiquement par ORIA.

**Résoudre (un outil)** — Au démarrage d'un agent, le runtime vérifie que tous les outils déclarés dans `tools_required` existent dans le Tool Registry. Si un outil requis est absent, l'agent ne démarre pas. Si un outil optionnel est absent, l'agent passe en `Degraded`.

## S

**Step** — Un cycle de raisonnement dans le StepBudget. Un appel LLM consomme 1 step. Un appel d'outil consomme 1 tool_call (compteur séparé).

**StepBudget** — Garde-fou tri-dimensionnel : `max_steps` (cycles de raisonnement), `max_tool_calls` (appels d'outils), `wall_clock_timeout` (durée maximale). Appliqué par le runtime Rust — non contournable par l'agent Python.

**Supervisor** — Acteur responsable du démarrage ordonné (13 phases), de la surveillance, et du redémarrage des autres acteurs. Orchestre l'arrêt graceful avec drain.

## T

**TaskState** — Machine d'état d'une tâche : `Submitted` → `Working` → `Completed` / `Failed` / `InputRequired` / `Canceled`.

**Trigger** — Source d'événement externe qui soumet automatiquement une tâche à un agent. 5 types : `cron`, `interval`, `oneshot`, `file_watch`, `webhook`.
