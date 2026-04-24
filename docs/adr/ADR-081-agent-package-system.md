# ADR-081 — Agent Package System : dossier auto-suffisant avec `agent.toml`

**Date :** 2026-04-24  
**Statut :** Accepté  
**Décideurs :** Nidal (CTO)

---

## Contexte

Installer un groupe d'agents liés (un director + ses workers) requiert aujourd'hui
N commandes `apollia agent install` séparées, suivies d'une configuration manuelle
des triggers via CLI. Il n'existe aucun concept de "package d'agents" dans le
runtime : les agents sont des entités unitaires sans lien déclaré entre eux.

Cette friction empêche la distribution d'agents sous forme de package cohérent
(ex. `veille-ia-agent` + `web-search-worker` + `synthesis-worker` + trigger cron).

---

## Décision

Introduire le concept d'**Agent Package** : un dossier auto-contenu décrit par un
fichier `agent.toml` à sa racine. Une seule commande installe l'ensemble.

### Format `agent.toml` (normatif)

```toml
[package]
name        = "mon-package"    # kebab-case, unique dans le runtime
version     = "1.0.0"          # semver
description = "..."
author      = "..."            # optionnel

[[agents]]
name  = "mon-director"
entry = "director.py"          # chemin relatif à la racine du package
role  = "director"             # "director" | "worker" | "assistant"

[[agents]]
name  = "mon-worker"
entry = "workers/worker.py"
role  = "worker"

[tools]
web = { enabled = true, ssrf_guard = true }   # optionnel

[[triggers]]
id             = "mon-trigger"
agent          = "mon-director"   # doit référencer un [[agents]].name
enabled        = true
on_busy        = "skip"           # "skip" | "queue" | "block"
input_template = "..."

[triggers.source]
type     = "cron"
schedule = "0 8 * * MON"

[pip]
packages = ["httpx>=0.27"]        # optionnel
```

### Invariants du format

- `[package].name` est unique dans le runtime — conflit → erreur fail-fast.
- Chaque `[[agents]].entry` doit exister et passer le duck-typing AIP.
- Les `[[triggers]].agent` doivent référencer un `[[agents]].name` déclaré.
- Le format trigger est identique à celui de `apollia-triggers::toml_config` — réutilisé sans duplication.

### Modèle de stockage

Deux nouvelles tables SQLite (migration 008) en relation avec `installed_agents` :

```
installed_packages  (name PK, version, root_path, manifest_json, timestamps)
package_agents      (package_name FK, agent_name FK, PRIMARY KEY composite)
```

Les agents d'un package restent dans `installed_agents` — source de vérité unique
pour Phase 11 du supervisor. La table `package_agents` sert uniquement au lien
package↔agents (liste, désinstallation groupée).

### Comportements clés

| Opération | Comportement |
|---|---|
| `install` | Validation fail-fast, copie du dossier, UPSERT idempotent en DB, injection triggers |
| `uninstall` | Supprime tous les agents du package, le venv partagé, les triggers injectés |
| Re-install | UPSERT : pas de doublon, triggers mis à jour |
| Agents sans package | Fonctionnent exactement comme avant — rétrocompatibilité totale |
| Boot (Phase 10.6) | Validation légère de `root_path` — si manquant, agents désactivés |

### Triggers

Les triggers déclarés dans `agent.toml` sont injectés en base SQLite à l'installation
via `parse_triggers_from_toml_str()` (existant dans `apollia-triggers::toml_config`).
Comportement idempotent (UPSERT sur l'`id` du trigger).

Les notifications (desktop, Discord, webhook) restent configurées via l'UI produit
et ne font pas partie du package — conformément à la décision de supprimer `apollia.toml`
global (ADR-079).

### `apollia.toml` global

Déprécié pour la configuration des agents et triggers. La config LLM backend reste
gérée via DB (ADR-079). Le fichier `apollia.toml` n'est plus le point d'entrée
des packages — chaque package est auto-suffisant.

---

## Alternatives rejetées

**Config inline dans le manifest Python** — mélange logique métier et config déploiement.
Les triggers et outils requis sont des préoccupations opérationnelles, pas du code agent.

**Registry centralisé (type npm/cargo)** — contredit le Principe #1 (Local-first) et
le Principe #2 (Zéro dépendance externe). La distribution est peer-to-peer (dossier,
Git clone) — pas via un serveur central.

**TOML global unique** — `apollia.toml` comme point de config unique crée du couplage
entre des agents non liés et rend la distribution impossible.

---

## Conséquences

- Les packages sont **distribuables** : un dossier ou un repo Git contient tout.
- La rétrocompatibilité est **totale** : les agents `.py` unitaires fonctionnent sans changement.
- Le runtime gagne une **Phase 10.6** légère de validation d'intégrité des packages au boot.
- L'UI desktop gagne un onglet **Packages** avec install/preview/uninstall.
- Le `HOW-TO-MAKE-AN-AGENT.md` devient la documentation de référence pour ce format.
