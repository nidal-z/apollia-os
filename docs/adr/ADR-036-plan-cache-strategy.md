# ADR-036 — Stratégie de cache de plans ORIA

**Date :** 2026-03-23
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 20

---

## Contexte

En mode Orchestré, ORIA appelle le LLM pour générer un `ExecutionPlan` pour chaque tâche soumise. Des tâches identiques ou quasi-identiques pour le même agent produisent le même plan, gaspillant des appels LLM et ajoutant de la latence. Avec l'augmentation du nombre de tâches répétitives (triggers cron, pipelines récurrents), ce coût devient significatif.

Nous avons besoin d'une stratégie de cache qui évite la génération redondante de plans tout en gérant correctement l'invalidation (changement de version agent, ajout/suppression d'outils).

## Décision

Nous adoptons un cache de plans en SQLite (`plan_cache.db`) avec clé de cache SHA-256 de `{agent_name}:{agent_version}:{sorted_tool_names}:{normalized_task_text}`. TTL 7 jours, max 1000 entrées, éviction LRU. Le cache est vérifié avant chaque appel `Reasoner::plan()`. Un cache hit émet `RuntimeEvent::PlanCacheHit` et réutilise le plan avec un nouveau `plan_id`.

## Alternatives considérées

### Option A — In-memory LRU cache (rejetée)
**Pour :** Lookup le plus rapide possible, zéro IO disque.
**Contre :** Perdu au redémarrage, pas de persistance, la mémoire croît de manière non bornée sur les serveurs long-running.

### Option B — No cache (actuel, rejetée)
**Pour :** Le plus simple, aucun bug d'invalidation possible.
**Contre :** Gaspille des appels LLM pour des tâches identiques, latence et coût plus élevés.

### Option retenue — SQLite persistent cache
**Pour :** Survit au redémarrage, stockage borné (1000 entrées max), pattern familier (même approche que les autres repositories), requêtable pour le debugging.
**Compromis acceptés :** Légèrement plus lent qu'un cache in-memory (mais SQLite est rapide pour un lookup par clé), nécessite une maintenance d'éviction.

## Conséquences

**Positives :**
- Réduction du coût LLM pour les tâches répétitives
- Latence réduite sur cache hit (pas d'appel LLM, plan immédiat)
- Observable via `RuntimeEvent::PlanCacheHit` (dashboard, logs, métriques)

**Négatives / Compromis :**
- Risque de staleness du cache (mitigé par `agent_version` dans la clé + TTL 7 jours)
- Base SQLite supplémentaire à maintenir (`plan_cache.db`)

**Neutres / À surveiller :**
- Monitorer le taux de cache hit pour valider l'efficacité de la stratégie
- Évaluer si le TTL de 7 jours est approprié selon les patterns d'usage réels
- Considérer un mécanisme d'invalidation manuelle via CLI (`apollia-os cache clear`)

## Principes architecturaux impactés
- Principe #1 — Local-first : cache stocké localement en SQLite, zéro donnée externalisée
- Principe #4 — Fail fast : un cache miss retombe gracieusement sur le `Reasoner` (aucun mode d'échec ajouté)

## Liens
- Story associée : STORY-231, STORY-232, STORY-233
