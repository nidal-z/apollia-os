# ADR-032 — Agent Install, Bundle Format & Package System

**Date :** 2026-03-17 (install) / 2026-04-17 (bundle format) / 2026-04-24 (package system)
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 16 (install) → Bloc 1.5 / Sprint 43 (bundle + packages)

---

## Contexte

### Persistance agents (Sprint 16)

Les agents Python étaient 100% éphémères : stockés dans un `HashMap` en mémoire dans `AgentRegistry`, perdus au shutdown. Les agents étaient le seul composant non-persisté dans `~/.apollia/`.

### Format de distribution (Sprint lancement v0.1.0)

La distribution des 4 assistants (`spec-assistant`, `dev-assistant`, `review-assistant`, `document-assistant`) nécessitait un format auto-descriptif : ces agents dépendent d'un module local `agents/assistants/shared/` que le simple `include_str!` ne pouvait pas transporter. Par ailleurs, un marketplace futur requiert un format indexable sans exécuter le Python.

### Système de packages (Sprint 43)

Installer un groupe d'agents liés (director + workers) requiert N commandes `agent install` séparées suivies d'une configuration manuelle des triggers. Il manquait un concept de "package d'agents" auto-contenu.

---

## Décisions

### 1 — Install & persistance dans `~/.apollia/agents/`

Modèle "agent install" avec copie locale et persistance SQLite :

1. Copie du bundle dans `~/.apollia/agents/<agent-name>/` lors de l'installation
2. Persistance des métadonnées dans `~/.apollia/agents.db` (SQLite)
3. Auto-reload au boot : le Supervisor charge tous les agents `enabled` depuis `agents.db`

**Flux d'installation :**

```
apollia-os agent install ./mon-agent/
  1. Valide manifest.toml parsable + agent.py présent
  2. Calcule SHA256 du bundle
  3. Copie récursive vers ~/.apollia/agents/<name>/
  4. Persiste manifest + métadonnées dans agents.db (enabled=true)
  5. Si runtime actif : enregistre dans AgentRegistry
  6. Émet RuntimeEvent::AgentInstalled
```

**Flux de boot :**

```
Supervisor::start()
  → Après AllReady : lit agents.db, filtre enabled=true
  → Pour chaque agent : load Python module → validate → register
  → Les agents en erreur sont logués (warning) mais n'empêchent pas le boot
```

**Commandes CLI :**

| Commande | Action |
|---|---|
| `agent install <path>` | Validation + copie + persistance + enregistrement |
| `agent uninstall <name>` | Supprime dossier + entrée DB + déregistre |
| `agent enable/disable <name>` | Toggle enabled dans DB |
| `agent update <name> <path>` | Remplace le bundle, revalide manifest |
| `agent list` | Affiche agents installés (enabled/disabled) + état runtime |

**Schéma SQLite (migration 007) :**

```sql
CREATE TABLE IF NOT EXISTS installed_agents (
    name            TEXT PRIMARY KEY,
    version         TEXT NOT NULL,
    install_path    TEXT NOT NULL,
    source_path     TEXT NOT NULL,
    manifest_json   TEXT NOT NULL,
    bundle_sha256   TEXT,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    installed_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### 2 — Format « agent bundle » : dossier standardisé

```
my-agent/
├── manifest.toml            ← OBLIGATOIRE — méta-fichier statique
├── agent.py                 ← OBLIGATOIRE — point d'entrée (manifest() + run())
├── lib/                     ← OPTIONNEL — modules locaux importables
│   ├── __init__.py
│   └── *.py
├── assets/                  ← OPTIONNEL — ressources read-only
├── requirements.txt         ← OPTIONNEL — deps pip (informatif v0.1.0)
└── README.md                ← OPTIONNEL
```

**Règles absolues :**
1. `manifest.toml` et `agent.py` sont les deux seuls fichiers obligatoires.
2. `agent.py` DOIT exposer `manifest()` et `async def run(task, ctx)` (contrat AIP inchangé).
3. Les modules locaux vivent **exclusivement** dans `lib/`. Import : `from lib import helpers`. Les imports à la racine (`from shared import …`) sont **interdits**.
4. Les ressources read-only vivent **exclusivement** dans `assets/`.

**`manifest.toml` — méta-fichier statique :**

```toml
[agent]
name = "spec-assistant"
version = "0.3.0"
description = "Feature spec consultant for Apollia OS projects"
license = "MIT"
authors = ["Nidal Zoumita <nidal.zoumita@gmail.com>"]
tags = ["assistant", "spec", "project-management"]

[agent.manifest]
tools_required = ["file_read", "file_write", "ask_user"]
tools_optional = ["bash_executor", "file_list", "memory_search"]
memory_namespace = "spec-assistant"

[agent.runtime]
python = ">=3.11,<3.14"
apollia_sdk = ">=0.3,<0.4"

[agent.dependencies]
packages = []

[agent.permissions]
network = false
filesystem = "workspace"   # "workspace" | "home" | "none"
```

**Chargement PyO3 (dans `apollia-aip::loader::load_agent_module`) :**
- Prepend `<install_path>/` à `sys.path` avant `importlib.util.spec_from_file_location`
- Charge `<install_path>/agent.py` avec `submodule_search_locations` pointant sur `<install_path>/`
- Nettoie `sys.path` après le load

**Conflit / mise à jour :** version ≤ existante → remplacement (backup `<name>.bak/`). Version > existante → rejet avec message « downgrade refusé ».

### 3 — Système de packages : `agent.toml` multi-agents

Un **Agent Package** est un dossier auto-contenu décrit par un fichier `agent.toml` :

```toml
[package]
name        = "mon-package"
version     = "1.0.0"
description = "..."

[[agents]]
name  = "mon-director"
entry = "director.py"
role  = "director"      # "director" | "worker" | "assistant"

[[agents]]
name  = "mon-worker"
entry = "workers/worker.py"
role  = "worker"

[tools]
web = { enabled = true, ssrf_guard = true }

[[triggers]]
id             = "mon-trigger"
agent          = "mon-director"
enabled        = true
on_busy        = "skip"
input_template = "..."

[triggers.source]
type     = "cron"
schedule = "0 8 * * MON"

[pip]
packages = ["httpx>=0.27"]
```

**Invariants :**
- `[package].name` unique dans le runtime — conflit → erreur fail-fast
- Chaque `[[agents]].entry` doit exister et passer le duck-typing AIP
- Les `[[triggers]].agent` référencent un `[[agents]].name` déclaré

**Schéma SQLite (migration 008) :**

```sql
-- installed_packages et package_agents (relation avec installed_agents)
CREATE TABLE IF NOT EXISTS installed_packages (
    name        TEXT PRIMARY KEY,
    version     TEXT NOT NULL,
    root_path   TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    installed_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS package_agents (
    package_name TEXT REFERENCES installed_packages(name),
    agent_name   TEXT REFERENCES installed_agents(name),
    PRIMARY KEY (package_name, agent_name)
);
```

**Comportements clés :**

| Opération | Comportement |
|---|---|
| `install` | Validation fail-fast, copie, UPSERT idempotent, injection triggers |
| `uninstall` | Supprime tous les agents du package et les triggers injectés |
| Re-install | UPSERT : pas de doublon, triggers mis à jour |
| Agents sans package | Fonctionnent exactement comme avant — rétrocompatibilité totale |
| Boot (Phase 10.6) | Validation légère de `root_path` — si manquant, agents désactivés |

Les triggers déclarés dans `agent.toml` sont injectés en base via `parse_triggers_from_toml_str()` (existant dans `apollia-triggers::toml_config`).

---

## Alternatives considérées

### Référence par chemin absolu (rejetée)

Le fichier source peut être déplacé ou supprimé entre deux sessions. Aucune garantie d'intégrité.

### Python wheel (`.whl`) (rejetée)

Impose `setuptools`/`pyproject.toml`, métadonnées PEP 621 complexes. Un utilisateur final doit pouvoir lire `manifest.toml` à l'œil nu.

### Registry centralisé type npm/cargo (rejetée)

Viole le Principe #1 (local-first) et le Principe #2 (nécessite un serveur externe). La distribution est peer-to-peer (dossier, Git clone).

### Config inline dans le manifest Python pour les packages (rejetée)

Mélange logique métier et config déploiement. Les triggers et outils requis sont des préoccupations opérationnelles, pas du code agent.

### TOML global unique `apollia.toml` pour les packages (rejetée)

Crée du couplage entre des agents non liés et rend la distribution impossible.

---

## Conséquences

**Positives :**
- Cohérence complète du modèle de persistance — tous les artefacts dans `~/.apollia/`
- UX fluide : installer une fois, oublier pour toujours
- Format auto-descriptif : `manifest.toml` indexable par un marketplace futur sans outillage Python
- Versioning propre : `[agent].version` + SHA256 permettent la détection d'updates
- Rétrocompatibilité totale : les agents `.py` unitaires fonctionnent sans changement
- Les packages sont distribuables : un dossier ou un repo Git contient tout

**Négatives / Compromis :**
- Duplication du fichier Python source (négligeable)
- Double source of truth entre `manifest()` Python et `[agent.manifest]` TOML (le TOML est informatif, le Python est la source réelle)
- `sys.path` prepending au load : micro-pollution de namespace Python (bornée, nettoyée après le load)
- Pas d'encryption ni de signature du bundle en v0.1.0

**Dette technique trackée :**
- Isolation des dépendances pip par agent (venv per-agent, v0.2+)
- Lint CI `manifest.toml` vs `manifest()` Python
- Signature cryptographique des bundles (prérequis marketplace)

---

## Principes architecturaux impactés

- Principe #1 — **Local-first** : tout vit dans `~/.apollia/`, zéro dépendance externe
- Principe #2 — **Zéro dépendance externe** : format TOML + arborescence standard Python, aucune dépendance nouvelle
- Principe #3 — **Contrat minimal** : 2 fichiers obligatoires, `manifest()` + `run()` inchangés
- Principe #4 — **Fail fast** : bundle invalide → rejet à l'install avec message explicite, pas d'état partiel

---

## Liens

- Stories : STORY-177 → STORY-183 (Sprint 16), Bloc 1.5 LAUNCH-BACKLOG, Sprint 43
- ADR-019 — AgentLoader trait (découplage runtime / PyO3)
- ADR-055 — Community registry (marketplace futur, dépend de ce format)
- ADR-061 — Permission Engine 3 layers (dépend de `[agent.permissions]`)
- Fichiers : `crates/apollia-tools/src/agent_repository.rs`, `crates/apollia-aip/src/loader.rs`, `crates/apollia-desktop/src/commands/agents.rs`
