# Publier dans le registre

Un Worker Agent validé localement peut être installé sur n'importe quel déploiement Apollia OS via une commande unique. La V1 du registre est un répertoire local (`agents/community/`). La V2 supportera l'installation directe depuis une URL Git.

---

## Installation depuis un chemin local

```bash
apollia-os agent install agents/community/csv-data-worker.py
```

Sortie attendue :

```
→ Validation du manifest...
✔ Manifest valide (name: csv-data-worker, version: 0.1.0)
→ Scan dangerous_tools_allowed...
✔ Aucun outil dangereux déclaré
→ Exécution des tests (pytest agents/tests/test_csv_data_worker.py)...
✔ 6/6 tests passés
✔ Agent "csv-data-worker" installé
```

L'installation est **idempotente** : si l'agent est déjà installé, le runtime le détecte et affiche `ℹ Agent déjà installé — aucune action`. Utilisez `--force` pour forcer la réinstallation.

---

## Les cinq étapes de validation

L'installateur effectue ces vérifications dans l'ordre. Un échec à n'importe quelle étape bloque l'installation :

| Étape | Vérification | Bloquant |
|---|---|---|
| 1 | Fichier `.py` existe à l'emplacement indiqué | Oui |
| 2 | `manifest()` est appelable et `run()` est une coroutine async | Oui |
| 3 | Champs obligatoires présents (`name`, `version`, `tools_required`) | Oui |
| 4 | `dangerous_tools_allowed: True` → avertissement affiché | Non (avertissement) |
| 5 | `pytest agents/tests/test_<name>.py` retourne 0 | Oui |

L'étape 5 peut être ignorée avec `--skip-tests` (non recommandé en production) :

```bash
apollia-os agent install ./csv-data-worker.py --skip-tests
# ⚠ Tests ignorés (--skip-tests) — installation non garantie
# ✔ Agent "csv-data-worker" installé
```

---

## Désinstaller un agent

```bash
apollia-os agent uninstall csv-data-worker
# ✔ Agent "csv-data-worker" désinstallé
```

Les données de mémoire dans le namespace `csv-data-worker` ne sont **pas supprimées** automatiquement. Pour purger la mémoire :

```bash
apollia-os memory purge --namespace csv-data-worker
```

---

## Structure du registre communautaire

```
agents/
├── bundled/                   ← 4 agents auto-installés au premier démarrage
│   ├── excel-worker.py
│   ├── csv-data-worker.py     ← votre agent rejoint ici après validation
│   ├── pdf-worker.py
│   ├── code-worker.py
│   └── manifest.json          ← liste des agents bundled
├── community/                 ← agents contributés par la communauté
│   ├── sql-worker.py
│   ├── git-worker.py
│   └── README.md              ← index du registre et guide de contribution
└── tests/
    ├── conftest.py
    ├── test_excel_worker.py
    ├── test_csv_data_worker.py
    └── test_sql_worker.py
```

---

## Critères d'acceptation communautaires

Pour qu'un agent soit accepté dans `agents/community/`, les trois critères suivants doivent être satisfaits :

**1. Séquence non-triviale**

L'agent réalise un workflow multi-étapes spécifique au domaine. Un wrapper autour d'un seul appel d'outil n'est pas un Worker Agent.

> `csv-data-worker` : détection encodage → détection séparateur → lecture pandas → inspection dtypes → calcul → export. Chaque étape dépend de la précédente.

**2. Garde-fou domaine hardcodé dans le code**

Au moins une règle de sécurité est encodée dans le code Python — pas seulement dans le SYSTEM_PROMPT.

> `csv-data-worker` : validation du format et de la taille dans `run()` avant le ReAct loop. `sql-worker` : rejet de toute requête non-SELECT sans opt-in explicite. `git-worker` : blocage de `git push --force` quelle que soit l'instruction LLM.

**3. Suite de tests**

`agents/tests/test_<name>.py` couvre au minimum un cas d'erreur. En pratique : test manifest + test guardrails statiques + test happy path + test(s) erreur domaine.

---

## Checklist avant contribution

```
[ ] manifest() retourne name, version, tools_required, description
[ ] dangerous_tools_allowed est déclaré explicitement (même si False)
[ ] Au moins un garde-fou domaine est dans le code Python (pas seulement dans SYSTEM_PROMPT)
[ ] pytest agents/tests/test_<name>.py sort avec code 0
[ ] apollia-os agent install agents/community/<name>.py réussit sur une installation propre
[ ] supports_a2a et skills sont déclarés si l'agent est composable
```

---

## Vérifier les agents A2A disponibles après installation

```bash
apollia-os agent list --supports-a2a
# A2A-capable agents (5):
#   csv-data-worker  [Active]
#     - read-csv    : Lit et retourne le contenu d'un CSV
#     - analyze-csv : Statistiques descriptives, groupby
#     - transform-csv: Filtrer, trier, exporter
#   excel-worker  [Active]
#     - read-excel  : Lit et retourne le contenu d'un classeur Excel
#   sql-worker  [Active]
#     - query-sql   : Exécute une requête SELECT sur une base SQLite
#     ...
```

Votre `csv-data-worker` est maintenant découvrable et invocable depuis n'importe quel Director Agent via `ctx.delegate("analyze-csv",...)`.

---

## Ce que vous avez construit

Au terme de ce chapitre, vous disposez d'un Worker Agent complet :

- Un `SYSTEM_PROMPT` structuré avec guardrails JAMAIS/TOUJOURS/RAISON
- Un `manifest()` avec skills A2A, packages pip, et métadonnées
- Une défense en profondeur : prompt + validation dans `run()` + interception des erreurs
- Une suite de tests couvrant manifest, guardrails statiques, happy path, et cas d'erreur
- Un agent installable sur n'importe quel déploiement Apollia OS

Ce pattern s'applique à n'importe quel domaine : Excel, PDF, SQL, Git, JSON, YAML, ou n'importe quelle librairie Python que vous voulez rendre accessible à vos agents.
