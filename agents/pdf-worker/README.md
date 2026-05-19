# pdf-worker

> Manipulation de fichiers PDF : extraction (texte/tables/forms), génération (Markdown → PDF), merge et split par plages — préserve metadata, sous-ensemble Markdown riche.

Worker Apollia OS standalone. Six skills A2A déterministes invocables par n'importe quel agent (custom director, Chat Libre via `a2a:*`, autres workers chaînés).

Décharge le LLM director : plus besoin de générer du code `pypdf`/`reportlab` à la volée. Le worker couvre **3 axes** :
- **Extraction** : texte par page, tables (pdfplumber), forms AcroForm
- **Génération** : Markdown → PDF avec page setup et metadata
- **Manipulation** : merge multiple PDFs, split par plages nommées ou page-par-page

## Skills exposés

| Skill ID | Description | Input principal |
|---|---|---|
| `pdf.read-text` | Texte par page + metadata (title, author, dates). | `path` |
| `pdf.read-tables` | Tables via pdfplumber + table_settings configurables. | `path` |
| `pdf.read-forms` | Champs AcroForm (text, checkbox, radio, dropdown, listbox, signature). | `path` |
| `pdf.render-from-markdown` | Markdown → PDF avec page_size, orientation, margins, metadata. | `markdown` ou `markdown_path`, `output_path` |
| `pdf.merge` | Concaténer N PDFs (≥2). | `paths`, `output_path` |
| `pdf.split` | Découper par plages nommées ou page-par-page. | `path`, `output_dir` |

**Format supporté** : `.pdf` uniquement.

## Dispatch multi-skills

Depuis Apollia v0.1.0 (2026-05-19), le runtime propage le `skill_id` invoqué dans `AIPTask.skill_id`. Le worker dispatche directement sur le full skill_id via `apollia.utils.a2a.extract_skill_id(task)`. Côté caller : aucune convention spéciale dans le payload — invoque simplement `ctx.a2a_invoke("<skill_id>", {...})`.


## Page range syntax

`"1-5,7,10-12"` : pages 1 à 5, plus page 7, plus pages 10 à 12. **Numérotation 1-based** (convention métier).

Le parser refuse :
- Ranges décroissantes (`"5-3"`) → `INVALID_PAGE_RANGE`
- Tokens non-numériques (`"abc"`) → `INVALID_PAGE_RANGE`
- Pages > `total_pages` → `PAGE_OUT_OF_RANGE`

Les doublons (`"1-3,2"`) sont silencieusement déduplicqués.

## Installation

```bash
apollia agent install ./pdf-worker
```

Au premier passage à `Active`, Apollia crée le venv et installe :
- `pypdf==5.1.0` (pure-Python, ~500 KB)
- `pdfplumber==0.11.4` + `pypdfium2` (binding C wheel, ~10-15 MB) + `pdfminer.six` (~2 MB)
- `reportlab==4.2.5` (pure-Python, ~5 MB, dépendance pillow tirée transitivement)
- `markdown==3.7` (pure-Python, ~200 KB)

Total venv : ~35 MB. Aucune dépendance native système (wheels précompilées).

## Usage

### Extraire texte par page

```python
import json

result = await ctx.a2a_invoke(
    "pdf.read-text",
    {"path": "/data/contrat.pdf", "page_range": "1-3"},
    timeout_secs=60,
)
data = json.loads(result["result"]["text"])
# data["pages"] → [{"page_num": 1, "text": "...", "char_count": 1234, "truncated": false}, ...]
# data["metadata"] → {"title": "...", "author": "...", "creation_date": "..."}
```

### Extraire tables

```python
result = await ctx.a2a_invoke(
    "pdf.read-tables",
    {
        "path": "/data/rapport-finance.pdf",
        "table_settings": {
            "vertical_strategy": "lines",
            "horizontal_strategy": "lines",
        },
    },
)
# data["tables"] → [{"page_num": 2, "table_index": 0, "rows": [["Année", "CA"], ...], "headers": [...]}, ...]
```

### Extraire form fields

```python
result = await ctx.a2a_invoke(
    "pdf.read-forms",
    {"path": "/data/formulaire-dpe.pdf"},
)
# data["fields"] → [{"name": "nom", "type": "text", "value": "", "readonly": false, "required": true, "max_length": 50}, ...]
```

### Markdown → PDF

```python
await ctx.a2a_invoke(
    "pdf.render-from-markdown",
    {
        "output_path": "/sandbox/rapport.pdf",
        "markdown": """
# Rapport mensuel

Voici **3 points** clés :

1. Croissance soutenue
2. Coûts maîtrisés
3. Marge en hausse

## Métriques

| Métrique | Valeur | Évolution |
|---|---|---|
| MRR | 12 000 € | +8% |
| Churn | 2.1% | -0.5pt |

> Note : les chiffres sont prévisionnels — révision le 1er du mois suivant.

```python
# Code échantillon
def hello():
    print("Apollia")
```

Pour plus de détails, voir [le dashboard](https://app.example.com/dash).
""",
        "page_size": "A4",
        "orientation": "portrait",
        "margins_cm": {"top": 2.5, "bottom": 2.5, "left": 2.5, "right": 2.5},
        "title": "Rapport mensuel",
        "author": "Apollia OS",
        "subject": "Métriques Mai 2026",
    },
)
```

### Merge

```python
await ctx.a2a_invoke(
    "pdf.merge",
    {
        "paths": ["/data/cover.pdf", "/data/body.pdf", "/data/annex.pdf"],
        "output_path": "/sandbox/full-doc.pdf",
        "metadata_from": 0,  # copie metadata depuis cover.pdf
    },
)
# data → {"output_path": "...", "merged_count": 3, "total_pages": 42, "file_size_bytes": 524288}
```

### Split par plages nommées

```python
await ctx.a2a_invoke(
    "pdf.split",
    {
        "path": "/data/contrat-100p.pdf",
        "output_dir": "/sandbox/parts/",
        "ranges": [
            {"name": "intro", "pages": "1-10"},
            {"name": "clauses", "pages": "11-80"},
            {"name": "annexes", "pages": "81-100"},
        ],
    },
)
# Génère intro.pdf, clauses.pdf, annexes.pdf dans /sandbox/parts/
```

### Split page-par-page (pas de ranges)

```python
await ctx.a2a_invoke(
    "pdf.split",
    {"path": "/data/5-pages.pdf", "output_dir": "/sandbox/pages/"},
)
# Génère page-001.pdf, page-002.pdf, ..., page-005.pdf
```

### Depuis Chat Libre

Les skills deviennent `a2a:pdf.read-text`, `a2a:pdf.merge`, etc.

### Eval

```bash
python3 pdf-worker/eval/run-eval.py
```

22+ cas couvrant les 6 skills + erreurs typées + page ranges + Markdown rendering.

## Markdown — features supportées

| Feature | Supporté en v0.1.0 |
|---|---|
| Headings h1–h6 | ✅ Tailles hardcodées (18/16/14/12/11/11 pt) |
| Paragraphes | ✅ Helvetica 11pt justifié |
| Bold `**texte**`, italic `*texte*` | ✅ |
| Inline code `` `code` `` | ✅ Courier 9pt |
| Code blocks fenced ` ``` ` | ✅ Courier 9pt, fond gris clair, bordure |
| Listes ordered/unordered | ✅ Imbrication 3 niveaux max |
| Tables (extension `tables`) | ✅ Header row en gras + fond bleu clair |
| Blockquotes `>` | ✅ Italique gris foncé, indenté |
| Horizontal rules `---` | ✅ Trait gris fin |
| Links `[texte](url)` | ✅ Cliquables, bleus soulignés |
| Strikethrough `~~texte~~` | ✅ |
| Images inline `![](url)` | ❌ Non supporté en v0.1.0 |
| CSS, math, footnotes complexes, custom HTML | ❌ Non supporté |

## Form fields — types détectés

| Type AcroForm | Mapping output |
|---|---|
| `/Tx` | `text` (avec `max_length` si défini) |
| `/Btn` flag 16 | `radio` |
| `/Btn` flag 17 | `pushbutton` |
| `/Btn` autres | `checkbox` |
| `/Ch` flag 18 | `dropdown` |
| `/Ch` autres | `listbox` (avec `options`) |
| `/Sig` | `signature` |
| Autres | `unknown` |

Chaque field expose : `name`, `type`, `value`, `readonly`, `required`, plus `options` (choice fields) et `max_length` (text fields) si applicables.

**XFA forms** (Adobe LiveCycle, format propriétaire) : refusés avec `UNSUPPORTED_FEATURE`.

## Codes d'erreur

| Code | Cause |
|---|---|
| `INVALID_PAYLOAD` | payload A2A vide |
| `MISSING_FIELD` | champ requis absent |
| `INVALID_TYPE` | mauvais type ou valeur hors whitelist |
| `INVALID_DATA` | paths vides, ranges vides, metadata_from hors plage |
| `FILE_NOT_FOUND` | fichier introuvable |
| `UNSUPPORTED_FORMAT` | extension ≠ `.pdf` |
| `UNSUPPORTED_FEATURE` | XFA form, etc. |
| `OUTPUT_EXISTS` | fichier existe + overwrite false |
| `TOO_LARGE` | fichier > 100 MB |
| `PARSE_ERROR` | PDF corrompu, pdfplumber error |
| `ENCRYPTED_PDF` | PDF protégé (non supporté en v0.1.0) |
| `INVALID_PAGE_RANGE` | syntaxe page_range invalide |
| `PAGE_OUT_OF_RANGE` | page > total_pages |
| `MARKDOWN_PARSE_ERROR` | Markdown invalide |
| `RENDER_ERROR` | reportlab a échoué |
| `EXECUTION_FAILED` | erreur inattendue |

## Configuration

Aucune. Tous les paramètres passent dans la payload.

## Limitations connues

- **OCR** : non. PDFs scannés → `read-text` renvoie texte vide. Composer avec `image-worker.ocr` après conversion pages → images (futur).
- **PDFs chiffrés** : non supportés en read. Password v0.2.0.
- **Markdown** : sous-ensemble (pas d'images inline, CSS, math, footnotes complexes, definition lists, custom HTML).
- **Annotations** (highlights, comments, stamps) : non exposées ni créables.
- **Bookmarks/TOC creation** : non. Préservation dans merge en best-effort uniquement.
- **Form filling** : read seulement (pas d'écriture de valeurs en v0.1.0).
- **XFA forms** (Adobe LiveCycle) : refusés.
- **PDF/A, PDF/X** (archivage/print) : output reportlab standard.
- **Compression/optimisation post-process** : non.
- **Taille fichier en read** : ≤ 100 MB.

## Sécurité

- `dangerous_tools_allowed = false`
- `tools_required = []`
- Aucun accès réseau (links dans Markdown sont rendus comme texte cliquable, jamais fetch)
- Markdown sanitisé : lib `markdown` safe par défaut, pas d'exécution arbitraire
- Validations internes : extension `.pdf`, taille ≤ 100 MB, refus PDFs chiffrés et XFA, page_range strict (1-based, croissant, bornes vérifiées), output_dir validation pour split

## License

MIT © Apollia OS

Voir [`CHANGELOG.md`](./CHANGELOG.md) pour l'historique des versions.
