# ADR-032 — Agent Install & Persistence dans ~/.apollia/agents/

**Date :** 2026-03-17
**Statut :** Accepte
**Decideur :** Nidal (solo)
**Sprint :** 16

---

## Contexte

Les agents Python sont aujourd'hui 100% ephemeres : stockes dans un `HashMap` en memoire dans `AgentRegistry`, perdus au shutdown. L'utilisateur doit recharger manuellement chaque agent a chaque redemarrage du runtime (CLI `agent start <path>` ou GUI file picker).

Tous les autres artefacts du systeme sont persistes dans `~/.apollia/` :

| Artefact | Persistance | Auto-reload au boot |
|---|---|---|
| Memoires | `memory.db` | oui |
| Tasks | `tasks.db` | oui |
| Plans | `plans.db` | oui |
| Triggers | `triggers.db` + TOML | oui |
| Pipelines | `pipelines.db` | oui |
| Audit trail | `audit.db` | oui |
| LLM calls | `llm_calls.db` | oui |
| **Agents** | **rien** | **non** |

Les agents sont le **seul composant non-persiste**, ce qui cree une incoherence dans le modele de donnees et une friction UX significative. De plus, les agents referencent un chemin absolu vers le fichier Python source, qui peut etre deplace ou supprime entre deux sessions.

## Decision

Nous adoptons un modele "agent install" avec copie locale et persistance SQLite :

1. **Copie du fichier Python** dans `~/.apollia/agents/<agent-name>/agent.py` lors de l'installation
2. **Persistance des metadonnees** dans `~/.apollia/agents.db` (SQLite) : nom, version, chemin installe, manifest JSON, enabled/disabled, date d'installation
3. **Auto-reload au boot** : le Supervisor charge tous les agents `enabled` depuis `agents.db` au demarrage
4. **Migration SQL** : `007_agent_tables.sql` dans `apollia-tools/migrations/`

### Flux d'installation

```
apollia-os agent install ./mon-agent.py
  1. Charge le module Python (PyO3 via AgentLoader)
  2. Valide manifest() + run() (duck typing AIP)
  3. Cree ~/.apollia/agents/<name>/
  4. Copie le fichier dans ~/.apollia/agents/<name>/agent.py
  5. Persiste manifest + metadonnees dans agents.db (enabled=true)
  6. Si runtime actif : enregistre dans AgentRegistry
  7. Emet RuntimeEvent::AgentInstalled
```

### Flux de boot

```
Supervisor::start()
  ...acteurs existants (EventBus → AgentRegistry → ToolRegistry → ...)...
  → Apres AllReady : lit agents.db, filtre enabled=true
  → Pour chaque agent : load Python module → validate → register
  → Les agents en erreur sont logues (warning) mais n'empechent pas le boot
```

### Nouvelles commandes

**CLI :**

| Commande | Action |
|---|---|
| `agent install <path>` | Copie + validation + persistance + enregistrement |
| `agent uninstall <name>` | Supprime dossier + entree DB + deregistre |
| `agent enable <name>` | Met enabled=true dans DB, charge si runtime actif |
| `agent disable <name>` | Met enabled=false, arrete l'agent si actif |
| `agent update <name> <path>` | Remplace le fichier Python, revalide manifest |
| `agent list` | Affiche agents installes (enabled/disabled) + etat runtime |

**Tauri IPC (GUI) :**

| Commande | Action |
|---|---|
| `install_agent(path)` | Meme flux, avec dialog natif file picker |
| `uninstall_agent(name)` | Avec confirmation dialog |
| `enable_agent(name)` / `disable_agent(name)` | Toggle dans la vue Agents |
| `update_agent(name, path)` | Re-import depuis file picker |

## Alternatives considerees

### Option A — Reference par chemin absolu (rejetee)
**Pour :** Simple a implementer, zero copie de fichier
**Contre :** Le fichier source peut etre deplace ou supprime entre deux sessions. Le chemin est dependant de la machine. Pas de garantie d'integrite. L'observabilite est perdue si le fichier disparait.

### Option B — Copie dans ~/.apollia/agents/ (retenue)
**Pour :** Autonomie complete du runtime, zero dependance au chemin source, coherent avec le modele de persistance de tous les autres artefacts, base pour le versioning futur
**Compromis acceptes :** Duplication du fichier Python (quelques Ko par agent), necessite une commande `update` pour resynchroniser si le source evolue

### Option C — Registry centralise type npm/pip (rejetee)
**Pour :** Decouverte d'agents, versioning semantique, partage communautaire
**Contre :** Over-engineering massif pour le MVP, viole le principe #1 local-first (necessite un serveur externe), complexite de packaging (dependencies Python, venv)

## Consequences

**Positives :**
- Coherence complete du modele de persistance — tous les artefacts vivent dans `~/.apollia/`
- UX fluide : installer une fois, oublier pour toujours
- Les tasks, memoires et historiques restent lies a un agent qui existe toujours dans le systeme
- Observabilite amelioree : `agent list` fonctionne meme runtime off (lecture directe agents.db)
- Base pour le versioning futur (garder N versions dans le dossier agent)

**Negatives / Compromis :**
- Duplication du fichier Python source (negligeable, quelques Ko par agent)
- Necessite une commande `update` pour resynchroniser avec le fichier source
- Migration SQL supplementaire (007)
- Le Supervisor doit gerer les erreurs de chargement au boot sans bloquer le demarrage (degraded graceful)

**Neutres / A surveiller :**
- Les agents multi-fichiers (avec imports relatifs, packages) necessiteront une evolution future : copier un dossier entier au lieu d'un seul fichier
- Le versioning (garder l'historique des mises a jour) est possible mais hors scope Sprint 16

## Principes architecturaux impactes

- Principe #1 — **Local-first** : renforce — tout vit dans `~/.apollia/`, zero dependance externe, zero appel reseau
- Principe #4 — **Fail fast** : le boot doit degrader gracieusement si un agent installe ne charge plus (log warning, continuer avec les autres agents)
- Principe #8 — **CLI humaine, API machine** : nouvelles commandes `install/uninstall/enable/disable/update` avec `--json` support

## Schema SQLite (agents.db)

```sql
-- Migration 007 — installed_agents
-- Idempotente : CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS

CREATE TABLE IF NOT EXISTS installed_agents (
    name            TEXT PRIMARY KEY,
    version         TEXT NOT NULL,
    install_path    TEXT NOT NULL,       -- ~/.apollia/agents/<name>/agent.py
    source_path     TEXT NOT NULL,       -- chemin original du fichier installe
    manifest_json   TEXT NOT NULL,       -- AgentManifest serialise en JSON
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    installed_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_installed_agents_enabled
    ON installed_agents(enabled);
```

## Liens

- Stories associees : STORY-177 a STORY-183 (Sprint 16)
- ADR precedent sur le sujet : aucun
- ADR-019 — AgentLoader trait : le trait existant est reutilise pour le chargement au boot
