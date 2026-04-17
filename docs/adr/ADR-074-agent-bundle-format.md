# ADR-074 — Format de distribution d'agent Python (« agent bundle »)

**Date :** 2026-04-17
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Bloc 1.5 du LAUNCH-BACKLOG (packaging release v0.1.0, 27 avril 2026)

---

## Contexte

Apollia OS distribue des agents Python via le desktop Tauri et le CLI
`apollia-os agent install <path>`. Jusqu'à présent, deux modes coexistaient sans
formalisation :

1. **Agents built-in** : embarqués dans le binaire desktop via `include_str!`
   (`crates/apollia-desktop/src/bundled_agents.rs`), extraits au 1er boot vers
   `~/.apollia/agents/<name>/agent.py`. Limité aux fichiers `.py` monolithiques —
   incapable de transporter les agents à plusieurs modules.
2. **Agents installés à la main** : `commands/agents.rs:353` copie `source_dir` →
   `~/.apollia/agents/<name>/` via `copy_python_tree`. Fonctionne pour les dossiers
   mais aucune convention ne définit *ce qui doit être dans le dossier*.

Le lancement v0.1.0 expose trois problèmes que ces approches ne résolvent pas :

1. **Les 4 assistants actuels** (`spec-assistant`, `dev-assistant`, `review-assistant`,
   `document-assistant`) dépendent d'un module local `agents/assistants/shared/` via
   `from shared import workspace_rules, discover_task_specs`. Un simple `include_str!`
   ne peut pas les transporter. L'ancienne extraction `~/.apollia/agents/<name>/agent.py`
   casse l'import (le fichier `shared.py` atterrit au mauvais niveau).
2. **Le marketplace post-MVP** (vision Nidal) nécessite un format **auto-descriptif**
   (nom, version, auteur, dépendances lisibles sans exécuter le Python) pour indexer,
   rechercher, et détecter les doublons ou mises à jour.
3. **Les agents de démo du bloc 1.4** (veille, mails, documents) sont attendus
   sous forme distribuable — on doit décider *maintenant* comment ils sont livrés
   pour ne pas refondre le format dans 2 sprints.

Trois alternatives ont été évaluées :

| Option | Principe | Jugement |
|---|---|---|
| A. Python wheel (`.whl`) | Réutiliser le standard PyPI | Trop lourd — un agent n'est pas un package pip, on n'a pas besoin de `setup.py`/`pyproject.toml`/metadata PEP 621. Outillage pip opaque pour un utilisateur final non-dev. |
| B. Zip avec `agent.py` inline | Tout aplati en un seul fichier | Casse la lisibilité du code, duplication de code partagé entre agents. |
| C. **Dossier standardisé + méta-fichier statique** | Convention minimale, lisible à l'œil nu, packageable en tarball | Retenu. |

---

## Décision

**Choix : C — dossier standardisé suivant la structure ci-dessous, optionnellement
empaquetable en `.tar.gz` pour le transport.**

### Structure d'un « agent bundle »

```
my-agent/                    ← nom = agent_name du manifest (kebab-case)
├── manifest.toml            ← OBLIGATOIRE — méta-fichier statique
├── agent.py                 ← OBLIGATOIRE — point d'entrée (manifest() + run())
├── lib/                     ← OPTIONNEL — modules locaux importables
│   ├── __init__.py
│   └── *.py
├── assets/                  ← OPTIONNEL — ressources read-only (prompts, templates, data)
│   └── ...
├── requirements.txt         ← OPTIONNEL — deps pip (informatif en v0.1.0, actif v0.2+)
└── README.md                ← OPTIONNEL — doc utilisateur
```

**Règles absolues :**

1. `manifest.toml` et `agent.py` sont les **deux seuls fichiers obligatoires**.
   Un bundle sans l'un des deux est invalide — rejet à l'install avec message clair.
2. `agent.py` DOIT exposer `manifest()` et `async def run(task, ctx)` conformément
   à l'AIP existant (Principe #3 — Contrat minimal). Aucun changement sémantique.
3. Les modules locaux vivent **exclusivement** dans `lib/`. L'import s'écrit :
   `from lib import helpers` ou `from lib.helpers import some_fn`. Les imports
   à la racine (`from shared import …`) sont **interdits** — incompatibles avec
   le nouveau chargement (voir §install ci-dessous).
4. Les ressources statiques read-only vivent **exclusivement** dans `assets/`.
   L'agent les lit via `Path(__file__).parent / "assets" / "foo.md"`.

### `manifest.toml` — méta-fichier statique

Le `manifest.toml` est la **source of truth pour les métadonnées** — lisible sans
exécuter le Python. Le `manifest()` Python reste la source of truth pour le
contrat AIP (tools, skills, step budget) car il peut dépendre du contexte
d'exécution. Les deux doivent être cohérents ; une incohérence détectée à l'install
produit un warning.

```toml
[agent]
name = "spec-assistant"
version = "0.3.0"
description = "Feature spec consultant for Apollia OS projects"
license = "MIT"
authors = ["Nidal Zoumita <nidal.zoumita@gmail.com>"]
homepage = "https://github.com/nidal-z/apollia-os"
tags = ["assistant", "spec", "project-management"]

[agent.manifest]
# Reproduction statique de ce que manifest() retourne, pour indexation sans exécuter Python.
tools_required = ["file_read", "file_write", "ask_user"]
tools_optional = ["bash_executor", "file_list", "memory_search"]
memory_namespace = "spec-assistant"

[agent.runtime]
# Contraintes d'exécution.
python = ">=3.11,<3.14"
apollia_sdk = ">=0.3,<0.4"

[agent.dependencies]
# Liste informative en v0.1.0 : erreur au load si un package manquant est importé.
# En v0.2+ : driver d'un venv per-agent (ADR-075 à venir).
packages = []

[agent.permissions]
# (Futur — §Conséquences) Capacités sensibles déclarées explicitement.
# Vide en v0.1.0, activé par ADR-061 (Permission Engine 3-layers).
network = false
filesystem = "workspace"   # "workspace" | "home" | "none"
```

### Processus d'installation

**Entrée :** un chemin qui est soit un dossier, soit une archive `.tar.gz` qui le
décompresse. Aucune autre forme supportée en v0.1.0.

**Étapes (implémentées dans `crates/apollia-tools/src/agent_repository.rs`) :**

1. **Validation structurelle :** `manifest.toml` parsable (`toml` crate), `agent.py`
   présent, `[agent].name` conforme regex `[a-z][a-z0-9-]{2,62}`.
2. **Hash du bundle :** SHA256 du tarball (ou du `.tar` reconstruit à partir du
   dossier) → stocké dans `InstalledAgent.bundle_sha256`. Permet la détection de
   doublons et de mises à jour.
3. **Conflict check :** si un agent de même `[agent].name` existe à version ≤,
   remplacement (avec backup `~/.apollia/agents/<name>.bak/`). À version >, rejet
   avec message « downgrade refusé, désinstaller d'abord ».
4. **Copie :** copie récursive du bundle vers `~/.apollia/agents/<name>/`.
   **Structure préservée à l'identique** — pas de flattening, pas de rewriting.
5. **Enregistrement :** insert/update dans la table SQLite `installed_agents`
   (`InstalledAgent { name, version, install_path, bundle_sha256, installed_at }`).

**Chargement par PyO3 (dans `apollia-aip::loader::load_agent_module`) :**

1. Lire `<install_path>/manifest.toml` pour récupérer `[agent].name`.
2. **Prepend** `<install_path>/` à `sys.path` **avant** le `importlib.util.spec_from_file_location`.
   C'est ce qui rend `from lib import helpers` résolvable : `lib/` est un sous-dossier
   de `<install_path>/`, donc accessible comme sub-package.
3. Charger `<install_path>/agent.py` avec l'option `submodule_search_locations` qui
   pointe sur `<install_path>/` — nécessaire pour que `from lib import …` marche
   même sans que `agent.py` lui-même soit dans un package.
4. Nettoyer `sys.path` après le load (protection contre la pollution si deux agents
   ont des modules `lib/` aux noms convergents).

### Migration des 4 assistants existants

La migration se fait dans le sprint de lancement (étape 3 du plan packaging) :

- `agents/assistants/spec-assistant.py` → `agents-distributable/spec-assistant/agent.py`
- `agents/assistants/shared/__init__.py` → `agents-distributable/spec-assistant/lib/__init__.py`
  (idem pour dev, review, document — chaque assistant a sa propre copie de `lib/`)
- Import dans `agent.py` : `from shared import foo` → `from lib import foo`
- Nouveau fichier : `agents-distributable/spec-assistant/manifest.toml` extrait de `manifest()`

Le dossier `agents/assistants/` historique **reste temporairement** pour éviter de
casser les tests Python existants (`agents/tests/test_spec_assistant.py`), sera
supprimé dans un sprint post-launch quand les tests seront migrés.

### Refus d'alternatives

**Python wheel (`.whl`) :** standard PyPI lourd, impose `setuptools`/`pyproject.toml`,
métadonnées PEP 621 complexes pour zéro bénéfice (on n'installe pas via pip, on ne
publie pas sur PyPI). Un utilisateur final doit pouvoir lire `manifest.toml` à l'œil
nu et comprendre.

**Zip avec agent.py inline :** perd la lisibilité du code (fichiers concaténés),
complique les revues de code communautaire. Encourage les agents géants en
monolithique au lieu de modulaires.

**Docker image par agent :** tue le local-first (Principe #1), overhead 200 MB
par agent pour un fichier Python de 200 lignes. Hors de scope.

**Intégrer les deps pip dans le bundle (vendoring) :** résout l'isolation mais
explose la taille (pandas seul = 50 MB). Préférable de gérer les deps via le
site-packages partagé v0.1.0 (voir packaging-design.md §1.b) et d'introduire
un venv per-agent en v0.2+ via ADR-075 à écrire.

---

## Conséquences

**Positives :**

- **Contrat minimal** (Principe #3) : 2 fichiers obligatoires, tout le reste est
  optionnel. Un agent trivial reste trivial à écrire.
- **Auto-descriptif** : `manifest.toml` rend les bundles indexables par un marketplace
  futur sans outillage Python côté serveur.
- **Versioning propre** : `[agent].version` + SHA256 permettent la détection d'updates
  et la protection contre les downgrades non désirés.
- **Futur-compatible** : `[agent.permissions]` et `[agent.dependencies]` sont déjà
  déclarés dans le format, activables sans breaking change quand les ADRs associés
  (ADR-061, ADR-075) seront implémentés.
- **Distribution flexible** : dossier pour le dev local, `.tar.gz` pour le transport
  par e-mail ou URL. Le même contenu.

**Négatives / Compromis :**

- **Migration requise** pour les 4 assistants existants — ~2 h de travail mécanique
  (structure + `from shared` → `from lib` + création `manifest.toml`). Traité dans
  le sprint de lancement.
- **Double source of truth** entre `manifest()` Python et `[agent.manifest]` TOML.
  Acceptable parce que le TOML est purement informatif/indexation ; le contrat
  réel reste `manifest()`. Un lint CI pourra détecter les divergences (story future).
- **`sys.path` prepending** au load d'un agent crée une micro-pollution de namespace
  Python. L'impact est borné (on nettoie après le load) mais reste une asymétrie avec
  le modèle des subinterpreters PEP 684 visé à long terme.
- **Pas d'encryption ni de signature du bundle en v0.1.0.** Un attaquant qui remplace
  un bundle sur un serveur peut injecter du code arbitraire. Non-problème en v0.1.0
  (distribution contrôlée manuellement) mais à traiter avant le marketplace
  (ADR futur : bundle signing).

**Dette technique trackée :**

- Story future : *ADR-075 — Isolation des dépendances pip par agent (venv per-agent)*.
  Active `[agent.dependencies].packages` à l'install.
- Story future : *Lint CI `manifest.toml` vs `manifest()` Python*.
- Story future : *Signature cryptographique des bundles* — prérequis marketplace.
- Story future : *Support `.tar.gz` en transport dans `apollia-os agent install`*.
  En v0.1.0 seuls les dossiers sont supportés.

---

## Principes architecturaux impactés

- **Principe #1 — Local-first :** Aucune donnée ne sort. Un bundle est un fichier
  local jusqu'à ce que l'utilisateur le distribue volontairement. Conforme.
- **Principe #2 — Zéro dépendance externe :** Format basé sur TOML + arborescence
  standard Python. Aucune dépendance nouvelle au runtime Apollia. Conforme.
- **Principe #3 — Contrat minimal :** 2 fichiers obligatoires, `manifest()` + `run()`
  restent le duck typing minimal. Le `manifest.toml` est un **enrichissement
  optionnel pour l'indexation** — pas une contrainte sur l'agent lui-même. Conforme.
- **Principe #4 — Fail fast :** Bundle invalide (manifest absent, import `from shared`,
  version non-parsable) → rejet à l'install avec message explicite, pas d'état partiel
  persisté. Conforme.

---

## Liens

- `docs/internal/packaging-design.md` — conception globale du packaging v0.1.0 (§3.5).
- `crates/apollia-tools/src/agent_repository.rs` — persistance SQLite des agents installés.
- `crates/apollia-aip/src/loader.rs` — chargeur PyO3 des modules agent.
- `crates/apollia-desktop/src/commands/agents.rs` — IPC install depuis UI.
- ADR-019 — AgentLoader trait (découplage runtime / PyO3).
- ADR-055 — Community registry (marketplace futur, dépend de ce format).
- ADR-061 — Permission Engine 3 layers (dépend de `[agent.permissions]`).
- Futur ADR-075 — Isolation des dépendances pip par agent.
