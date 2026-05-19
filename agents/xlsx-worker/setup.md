# Setup — xlsx-worker v0.1.0

## Prérequis

- Apollia OS **v0.1.0+** installé et fonctionnel (`apollia --version`)
- Python 3.13 (bundled avec Apollia — rien à installer côté utilisateur)

### Dépendances Python (installées automatiquement dans le venv)

| Package | Version | Rôle |
|---|---|---|
| `openpyxl` | `3.1.5` | Lecture/écriture .xlsx, styles, conditional formatting |
| `pandas` | `2.2.3` | Skill `xlsx.read-as-dataframe` (dtype inference + slicing) |

`pandas` tire automatiquement `numpy==2.x`, `python-dateutil`, `pytz`, `tzdata`. **Taille totale du venv : ~50 MB.**

### Dépendances natives système

Aucune. `openpyxl` est pure Python ; `pandas`/`numpy` fournissent des wheels précompilées pour macOS (arm64/x86_64), Linux (x86_64/aarch64), Windows.

## Installation

```bash
apollia agent install ./xlsx-worker
```

Cette commande :
1. Valide `agent.toml` et le code Python (duck typing : `agent` au niveau module).
2. Copie le dossier vers `~/.apollia/agents/packages/xlsx-worker/`.
3. Enregistre le worker dans `~/.apollia/agents.db`.

Au premier passage à `Active`, le runtime crée un venv dédié :

```
~/.apollia/venvs/xlsx-worker/venv/
  ├─ openpyxl==3.1.5
  └─ pandas==2.2.3 (+ numpy, dateutil, pytz, tzdata)
```

La première installation peut prendre 10-30 secondes (téléchargement pip de numpy + pandas).

## Vérification

```bash
apollia agent list | grep xlsx-worker
```

État attendu : `Active`. Si `Degraded` ou `Failed`, voir Troubleshooting.

Test rapide via eval :

```bash
python3 ~/.apollia/agents/packages/xlsx-worker/eval/run-eval.py
```

Sortie attendue : `17/17 cases passed (100%)`.

## Désinstallation

```bash
apollia agent uninstall xlsx-worker
```

Cela retire :
- L'enregistrement SQLite (`agents.db`, `packages.db`)
- Le dossier `~/.apollia/agents/packages/xlsx-worker/`

Le venv (`~/.apollia/venvs/xlsx-worker/`) n'est pas supprimé automatiquement (50 MB sur disque) :

```bash
rm -rf ~/.apollia/venvs/xlsx-worker
```

## Troubleshooting

### Le worker reste `Initializing` longtemps

Premier boot : pip télécharge numpy + pandas (~30-40 MB compilés). Surveiller :

```bash
tail -f ~/.apollia/logs/runtime.log | grep xlsx-worker
```

Si bloqué > 2 min : vérifier la connectivité PyPI (`pip config list`) et l'espace disque (`df -h ~/.apollia/`).

### `INVALID_PAYLOAD` à chaque appel

Le director doit utiliser `ctx.a2a_invoke(skill_id, payload_dict, ...)` avec un `payload_dict` non vide. Si le payload est passé sous forme de string, le runtime ne le wrappe pas en `DataPart`.

### `MISSING_SKILL_ID`

Le runtime n'a pas propagé de `skill_id` jusqu'au worker. Le worker xlsx-worker est multi-skills : il a besoin du `skill_id` pour dispatcher. Invoquer via `ctx.a2a_invoke("<skill>", payload)` pour que le runtime propage automatiquement.

### `UNKNOWN_SKILL_ID`

Le `skill_id` reçu n'est pas dans la liste : `"xlsx.read", "xlsx.read-as-dataframe", "xlsx.write", "xlsx.append-rows", "xlsx.update-cells"`. Vérifier l'orthographe côté caller.

### `UNSUPPORTED_FORMAT`

Le worker supporte uniquement `.xlsx`. Pour `.xls` (Excel 97-2003), `.xlsm` (macros) ou `.xlsb` (binaire), retour `UNSUPPORTED_FORMAT`. Convertir au préalable côté caller via LibreOffice CLI ou un autre worker.

### `PARSE_ERROR`

Fichier .xlsx corrompu, tronqué, ou chiffré. Vérifier :

```bash
unzip -t /tmp/file.xlsx       # un .xlsx est un zip — l'intégrité du zip doit être OK
```

### `TOO_LARGE`

Fichier > 100 MB (en lecture) ou > 1 048 576 lignes (limite Excel) en écriture. Pour les très gros fichiers, considérer un worker streaming dédié (futur).

### `INVALID_STYLE: Couleur hex invalide`

Format attendu : `"RRGGBB"` ou `"AARRGGBB"` (6 ou 8 hex chars). Préfixe `#` toléré. Pas de noms de couleurs (`"red"`, etc.).

### `STYLE_NOT_FOUND`

Le `style` référencé dans `styles_apply` ou `conditional_formatting` n'existe pas dans `named_styles`. Vérifier l'orthographe et la portée (les styles sont déclarés au niveau racine, pas par feuille).

### `DTYPE_COERCION_FAILED` (xlsx.read-as-dataframe)

Le `dtypes_hint` demande un type incompatible avec les données (ex: `"int"` sur une colonne contenant `"abc"`). Adapter le hint ou pré-nettoyer le fichier.

### Output identique à l'entrée (en `xlsx.write` avec overwrite)

Vérifier qu'aucun process tiers (Excel ouvert, indexer macOS) ne lock le fichier au moment du save. Sur Windows surtout : fermer le fichier dans Excel avant l'appel.

### Logs détaillés

```bash
RUST_LOG=apollia=debug apollia-os start --foreground 2>&1 | grep xlsx-worker
```
