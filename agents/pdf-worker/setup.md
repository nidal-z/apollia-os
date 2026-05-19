# Setup — pdf-worker v0.1.0

## Prérequis

- Apollia OS **v0.1.0+** installé et fonctionnel (`apollia --version`)
- Python 3.13 (bundled avec Apollia)

### Dépendances Python (installées automatiquement dans le venv)

| Package | Version | Rôle |
|---|---|---|
| `pypdf` | `5.1.0` | Lecture texte + manipulation (merge/split/forms) |
| `pdfplumber` | `0.11.4` | Extraction tables (gestion fine du layout) |
| `reportlab` | `4.2.5` | Génération PDF programmable |
| `markdown` | `3.7` | Parsing Markdown → HTML |

`pdfplumber` tire `pypdfium2` (binding C précompilé, ~12 MB) et `pdfminer.six` (~2 MB). `reportlab` tire `pillow` (~3 MB). **Taille totale du venv : ~35 MB.**

### Dépendances natives système

Aucune. `pypdfium2` et `pillow` fournissent des wheels précompilées pour macOS (arm64/x86_64), Linux (x86_64/aarch64), Windows.

## Installation

```bash
apollia agent install ./pdf-worker
```

Cette commande :
1. Valide `agent.toml` et le code Python (duck typing).
2. Copie vers `~/.apollia/agents/packages/pdf-worker/`.
3. Enregistre dans `~/.apollia/agents.db`.

Au premier passage à `Active` :

```
~/.apollia/venvs/pdf-worker/venv/
  ├─ pypdf==5.1.0
  ├─ pdfplumber==0.11.4 (+ pypdfium2, pdfminer.six)
  ├─ reportlab==4.2.5 (+ pillow)
  └─ markdown==3.7
```

Installation : 10-20 secondes selon connexion.

## Vérification

```bash
apollia agent list | grep pdf-worker
```

État attendu : `Active`.

Test rapide via eval :

```bash
python3 ~/.apollia/agents/packages/pdf-worker/eval/run-eval.py
```

Sortie attendue : 22+/22+ cases passed (100%).

## Désinstallation

```bash
apollia agent uninstall pdf-worker
rm -rf ~/.apollia/venvs/pdf-worker
```

## Troubleshooting

### Le worker reste `Initializing` longtemps

Pip télécharge pypdfium2 + reportlab. Surveiller :

```bash
tail -f ~/.apollia/logs/runtime.log | grep pdf-worker
```

Si bloqué > 2 min : connectivité PyPI, espace disque (`df -h ~/.apollia/`).

### `MISSING_SKILL_ID`

Le runtime n'a pas propagé de `skill_id` jusqu'au worker. Le worker pdf-worker est multi-skills : il a besoin du `skill_id` pour dispatcher. Invoquer via `ctx.a2a_invoke("<skill>", payload)` pour que le runtime propage automatiquement.

### `UNKNOWN_SKILL_ID`

Le `skill_id` reçu n'est pas dans la liste : `"pdf.read-text", "pdf.read-tables", "pdf.read-forms", "pdf.render-from-markdown", "pdf.merge", "pdf.split"`. Vérifier l'orthographe côté caller.

### `UNSUPPORTED_FORMAT`

Le worker supporte uniquement `.pdf`. Pour `.docx`/`.odt`/`.epub` → utiliser un worker dédié (docx-worker pour Word, autre futur).

### `ENCRYPTED_PDF`

PDF protégé par password. Non supporté en v0.1.0. Pour décrypter en amont : `qpdf --password=XXX --decrypt input.pdf output.pdf` puis passer `output.pdf` au worker.

### `UNSUPPORTED_FEATURE: XFA forms`

XFA (Adobe LiveCycle) est un format propriétaire de form, distinct de AcroForm. Non supporté en v0.1.0. Si tu rencontres ce cas régulièrement, convertir le PDF en AcroForm pur via Adobe Acrobat ou un outil tiers.

### `INVALID_PAGE_RANGE`

Vérifier la syntaxe : `"1-5,7,10-12"`. Règles :
- Numérotation 1-based (`"0"` est invalide)
- Ranges croissantes (`"5-3"` est invalide)
- Pas de tokens non-numériques (`"abc"` est invalide)
- Pas de tokens vides (`",1"` est invalide)

### `PAGE_OUT_OF_RANGE`

La page demandée dépasse `total_pages` du PDF. Vérifier le nombre de pages :

```python
result = await ctx.a2a_invoke("pdf.read-text", {"path": "/tmp/x.pdf"})
total = json.loads(result["result"]["text"])["total_pages"]
```

### `MARKDOWN_PARSE_ERROR`

Le Markdown produit un HTML mal formé. Causes possibles :
- HTML inline dans le Markdown avec balises non fermées
- Tables mal formées (colonnes manquantes)
- Code blocks non fermés

Tester le Markdown avec `python -c "import markdown; print(markdown.markdown(open('x.md').read(), extensions=['tables', 'fenced_code']))"` pour voir le HTML produit.

### `RENDER_ERROR`

reportlab a échoué pendant `doc.build()`. Causes possibles :
- Page trop petite pour contenir le contenu (margins_cm trop grandes)
- Table trop large (column widths cumulés > page width)
- Caractères Unicode non supportés par la police (rare avec Helvetica/Courier)

### `TOO_LARGE`

Fichier > 100 MB en read. Options :
- Découper en plus petits PDFs en amont (split par 50 pages)
- Augmenter la limite dans une version custom du worker (modifier `MAX_FILE_BYTES`)

### Texte vide après `read-text`

PDF scanné (image-based) — pas de couche texte à extraire. Solution : convertir les pages en images puis utiliser `image-worker.ocr` (futur worker).

### `pdfplumber` lent sur gros PDF

Pour des PDFs ≥ 50 pages avec beaucoup de tables, l'extraction peut prendre 10-30 secondes. Le `wall_clock_secs: 300` du worker laisse 5 min, mais en pratique :
- Restreindre via `page_range` aux pages où sont les tables
- Désactiver `include_headers` si non utilisé (gain marginal)

### Logs détaillés

```bash
RUST_LOG=apollia=debug apollia-os start --foreground 2>&1 | grep pdf-worker
```
