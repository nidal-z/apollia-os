# Brique — CLI

> Interface en ligne de commande d'Apollia OS.
> Binaire : `apollia-os`. Crate : `apollia-cli` (clap v4 derive).
> Convention : noun-verb (ADR-008), `--json` sur toutes les commandes, TTY auto-détecté.

---

## Vue d'ensemble

La CLI est le point d'entrée principal pour interagir avec le runtime Apollia OS. Elle communique avec le runtime via un client HTTP Unix socket (`RuntimeClient`). Toutes les commandes supportent le flag `--json` pour une sortie structurée (intégration machine).

**Fichier principal** : `crates/apollia-cli/src/main.rs`
**Client HTTP** : `crates/apollia-cli/src/client.rs`

---

## Commandes — Runtime

### `apollia-os start`

Démarre le runtime (Supervisor, EventBus, agents, API server).

### `apollia-os stop`

Arrête le runtime proprement (graceful shutdown, drain 30s).

### `apollia-os status`

Affiche l'état du runtime (santé, agents actifs, tâches en cours).

---

## Commandes — Agents

### `apollia-os agent list`

Liste tous les agents enregistrés avec leur état.

### `apollia-os agent start <name>`

Démarre un agent spécifique.

### `apollia-os agent stop <name>`

Arrête un agent (transition vers `Stopped`).

### `apollia-os agent info <name>`

Détails d'un agent (manifest, état, tâches).

---

## Commandes — Tâches

### `apollia-os run <agent> [--input <text>]`

Soumet une tâche à un agent et attend le résultat.

### `apollia-os task list`

Liste les tâches (filtrage : `--pending-approval`).

### `apollia-os task status <id>`

Statut d'une tâche spécifique.

### `apollia-os task cancel <id>`

Annule une tâche en cours.

### `apollia-os task resume <id> [--approve|--reject]`

Reprend une tâche en attente d'approbation (HITL).

### `apollia-os task inspect <id>`

Inspecte les plans d'exécution d'une tâche (lecture directe SQLite, sans runtime).

---

## Commandes — Onboarding

**Fichier** : `crates/apollia-cli/src/commands/onboard.rs`

### `apollia-os onboard`

Lance l'onboarding complet. Soumet une tâche à l'agent `onboarding-agent` et attend le résultat. Le runtime doit être démarré au préalable.

```bash
apollia-os onboard
#  -> Onboarding task abc123 submitted
#  ... conversation ...
#  * Onboarding completed in 45.2s
```

### `apollia-os onboard --topic <topic>`

Re-déclenche l'onboarding sur un domaine spécifique. L'agent concentre la conversation sur ce seul domaine au lieu de couvrir les 5.

```bash
apollia-os onboard --topic preferences
apollia-os onboard --topic tools --json
```

**Topics valides** : `identity`, `preferences`, `tools`, `domain`, `agents`.

Un topic invalide retourne une erreur avec la liste des valeurs acceptées :

```
Error: invalid topic 'invalid', valid topics: identity, preferences, tools, domain, agents
```

---

## Commandes — Outils

### `apollia-os tools list`

Liste les outils disponibles (natifs + MCP).

### `apollia-os tools describe <name>`

Description détaillée d'un outil.

---

## Commandes — Mémoire

### `apollia-os memory inspect`

Explore le contenu des stores mémoire (épisodique, sémantique, procédural).

---

## Commandes — LLM

### `apollia-os llm status`

État des backends LLM configurés.

### `apollia-os llm ping`

Vérifie la connectivité d'un backend LLM.

### `apollia-os llm chat`

Session de chat interactive avec un backend LLM.

### `apollia-os model list`

Liste les modèles locaux dans `~/.apollia/models/` (lecture locale, sans runtime).

---

## Commandes — Triggers

### `apollia-os trigger list|status|fire|enable|disable|logs|reload`

Gestion des triggers (cron, interval, file watch, webhook).

---

## Commandes — Pipelines

### `apollia-os pipeline list|run|runs|status`

Gestion des pipelines multi-agent.

---

## Commandes — Notifications

### `apollia-os notify test|list|logs`

Gestion des notifications.

---

## Commandes — Audit

### `apollia-os audit list|stats`

Consultation du journal d'audit des appels outils.

---

## Conventions

- **Noun-verb** : `apollia-os agent start`, pas `apollia-os start-agent` (ADR-008)
- **`--json`** : Sortie JSON structurée sur toutes les commandes
- **TTY auto-détecté** : Couleurs et formatage adaptatif (Principe #8)
- **Exit codes POSIX** : 0 = succès, 1 = erreur générale, codes spécifiques pour runtime/task errors

---

## Liens

- [API HTTP Reference](API-HTTP-Reference.md)
- [Guide Onboarding](Agents-Onboarding-Guide.md)
- [Architecture — Vue d'ensemble](Architecture-Vue-Ensemble.md)
