# Setup — docx-worker v0.1.0

## Prérequis

- Apollia OS **v0.1.0+** installé et fonctionnel (`apollia --version`)
- Python 3.13 (bundled avec Apollia)

### Dépendances Python (installées automatiquement dans le venv)

| Package | Version | Rôle |
|---|---|---|
| `python-docx` | `1.1.2` | Lecture/écriture .docx native, styles, tables, sections |
| `docxtpl` | `0.18.0` | Skill `docx.render-from-template` (Jinja2 sur .docx) |

`python-docx` tire `lxml` (wheel précompilée par OS). `docxtpl` tire `Jinja2==3.1.4`, `MarkupSafe`, `six`. **Taille totale du venv : ~15 MB.**

### Dépendances natives système

Aucune. `lxml` fournit des wheels précompilées pour macOS (arm64/x86_64), Linux (x86_64/aarch64), Windows.

## Installation

```bash
apollia agent install ./docx-worker
```

Cette commande :
1. Valide `agent.toml` et le code Python (duck typing : `agent` au niveau module).
2. Copie vers `~/.apollia/agents/packages/docx-worker/`.
3. Enregistre dans `~/.apollia/agents.db`.

Au premier passage à `Active` :

```
~/.apollia/venvs/docx-worker/venv/
  ├─ python-docx==1.1.2 (+ lxml binding C)
  └─ docxtpl==0.18.0 (+ Jinja2, MarkupSafe)
```

Installation : 5-15 secondes selon connexion.

## Vérification

```bash
apollia agent list | grep docx-worker
```

État attendu : `Active`.

Test rapide via eval :

```bash
python3 ~/.apollia/agents/packages/docx-worker/eval/run-eval.py
```

Sortie attendue : `22/22 cases passed (100%)`.

## Désinstallation

```bash
apollia agent uninstall docx-worker
rm -rf ~/.apollia/venvs/docx-worker
```

## Troubleshooting

### Le worker reste `Initializing` longtemps

Pip télécharge lxml + Jinja2. Surveiller :

```bash
tail -f ~/.apollia/logs/runtime.log | grep docx-worker
```

Si bloqué > 2 min : connectivité PyPI (`pip config list`), espace disque (`df -h ~/.apollia/`).

### `MISSING_SKILL_ID`

Le runtime n'a pas propagé de `skill_id` jusqu'au worker. Le worker docx-worker est multi-skills : il a besoin du `skill_id` pour dispatcher. Invoquer via `ctx.a2a_invoke("<skill>", payload)` pour que le runtime propage automatiquement.

### `UNKNOWN_SKILL_ID`

Le `skill_id` reçu n'est pas dans la liste : `"docx.read", "docx.write", "docx.append-section", "docx.extract-tables", "docx.render-from-template"`. Vérifier l'orthographe côté caller.

### `UNSUPPORTED_FORMAT`

Le worker supporte uniquement `.docx`. `.doc` (Word 97-2003) et `.docm` (macros) → refusés. Pour convertir un `.doc` ou `.docm` : LibreOffice CLI (`libreoffice --convert-to docx`) en amont.

### `PARSE_ERROR`

Fichier .docx corrompu, tronqué, ou chiffré. Vérifier :

```bash
unzip -t /tmp/file.docx
```

(un .docx est un zip — l'intégrité du zip doit être OK)

### `UNDEFINED_VARIABLES` (render-from-template)

Le template référence une variable Jinja2 absente du `context`. Soit ajouter la variable au context, soit passer `strict_undefined: false` pour laisser le placeholder littéral dans le doc rendu.

Pour découvrir les variables attendues sans `context` :

```python
from docxtpl import DocxTemplate
tpl = DocxTemplate("/templates/contrat.docx")
print(tpl.get_undeclared_template_variables(context={}))
# {'client', 'commande', 'signature', ...}
```

### `TEMPLATE_RENDER_ERROR`

Erreur Jinja2 dans le template : syntaxe invalide (`{% for x %}` sans `endfor`), expression mal formée, ou filtre inconnu. Vérifier le template dans Word et tester avec un contexte minimal.

### `IMAGE_NOT_FOUND` / `IMAGE_UNSUPPORTED`

`type: image` block exige un `path` existant. Formats supportés : `.png`, `.jpg`, `.jpeg`, `.gif`, `.tiff`, `.tif`, `.bmp`. SVG non supporté (rasterisation requise en amont).

### `INVALID_STYLE: Couleur hex invalide`

Format attendu : `"RRGGBB"` ou `"AARRGGBB"` (6 ou 8 chars hex). Préfixe `#` toléré. Pas de noms de couleurs (`"red"` n'est pas accepté en `color` — pour `highlight`, voir le mapping ci-dessous).

### `INVALID_BLOCK`

Type de block non reconnu ou structure mal formée. Types valides : `paragraph`, `heading`, `table`, `list`, `image`, `page_break`, `section_break`.

### Highlights : couleurs reconnues

Le champ `highlight` dans `font` accepte ces noms (case-insensitive) :
`yellow`, `green`, `cyan`, `magenta`, `blue`, `red`, `darkblue`, `darkcyan`, `darkgreen`, `darkmagenta`, `darkred`, `darkyellow`, `darkgray`, `lightgray`, `black`, `white`.

Autres valeurs : ignorées silencieusement.

### Header/footer placeholders non substitués

Si `{my_var}` reste tel quel dans le doc rendu, c'est qu'il n'est pas reconnu. Vérifier la liste des 10 placeholders supportés dans le README. Pour des templates plus dynamiques, utiliser plutôt `docx.render-from-template` (Jinja2 complet).

### Output identique à l'entrée

Vérifier qu'aucun process tiers (Word ouvert) ne lock le fichier au moment du save. Sur Windows surtout.

### Logs détaillés

```bash
RUST_LOG=apollia=debug apollia-os start --foreground 2>&1 | grep docx-worker
```
