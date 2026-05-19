# docx-worker

> Manipulation de fichiers Word (.docx) : lire, écrire, ajouter, extraire des tables, rendre des templates Jinja2 — préserve styles, sections, headers/footers.

Worker Apollia OS standalone. Cinq skills A2A déterministes invocables par n'importe quel agent (custom director, Chat Libre via `a2a:*`, autres workers chaînés).

Décharge le LLM director : génération de rapports/contrats/lettres-types avec styles avancés et placeholders Jinja2 sans générer de code python-docx à la volée.

## Skills exposés

| Skill ID | Description | Input principal |
|---|---|---|
| `docx.read` | Lit le document (blocs ordonnés, sections, hyperlinks, styles). | `path` |
| `docx.write` | Crée un .docx multi-section avec styles avancés. | `output_path`, `sections` |
| `docx.append-section` | Ajoute des blocs en fin de document. | `path`, `blocks` |
| `docx.extract-tables` | Extraction ciblée des tables uniquement. | `path` |
| `docx.render-from-template` | Rend un template Jinja2 (docxtpl) avec contexte. | `template_path`, `output_path`, `context` |

**Format supporté** : `.docx` uniquement. `.doc`, `.docm`, `.rtf` → `UNSUPPORTED_FORMAT`.

## Dispatch multi-skills

Depuis Apollia v0.1.0 (2026-05-19), le runtime propage le `skill_id` invoqué dans `AIPTask.skill_id`. Le worker dispatche directement sur le full skill_id via `apollia.utils.a2a.extract_skill_id(task)`. Côté caller : aucune convention spéciale dans le payload — invoque simplement `ctx.a2a_invoke("<skill_id>", {...})`.


## Installation

```bash
apollia agent install ./docx-worker
```

Au premier passage à `Active`, Apollia crée le venv et installe :
- `python-docx==1.1.2` (manipulation native .docx + lxml binding C wheel)
- `docxtpl==0.18.0` (rendu de templates Jinja2 sur .docx + Jinja2)

Total venv : ~15 MB.

## Usage

### Lire

```python
import json

result = await ctx.a2a_invoke(
    "docx.read",
    {"path": "/data/rapport.docx", "include_runs": True},
    timeout_secs=60,
)
data = json.loads(result["result"]["text"])
# data["sections"][0]["blocks"] → [{"type": "heading", "text": "...", "level": 1}, ...]
# data["hyperlinks"] → [{"text": "...", "url": "https://..."}]
# data["styles_used"] → ["Heading 1", "Normal", "List Bullet"]
```

### Écrire un contrat avec styles avancés

```python
await ctx.a2a_invoke(
    "docx.write",
    {
        "output_path": "/sandbox/contrat.docx",
        "named_styles": {
            "title": {
                "font": {"size_pt": 18, "bold": True, "color": "1F4E78"},
                "paragraph": {"alignment": "center", "space_after_pt": 12},
            },
            "body": {
                "font": {"size_pt": 11},
                "paragraph": {"alignment": "justify", "line_spacing": 1.15, "space_after_pt": 6},
            },
            "highlight": {
                "font": {"color": "9C0006", "bold": True, "highlight": "yellow"},
            },
            "header_table": {
                "table": {
                    "borders": {
                        "top":    {"style": "single", "size_pt": 1.5, "color": "1F4E78"},
                        "bottom": {"style": "single", "size_pt": 1.5, "color": "1F4E78"},
                        "left":   {"style": "single", "size_pt": 0.5, "color": "000000"},
                        "right":  {"style": "single", "size_pt": 0.5, "color": "000000"},
                        "inside_h": {"style": "single", "size_pt": 0.5, "color": "BFBFBF"},
                        "inside_v": {"style": "single", "size_pt": 0.5, "color": "BFBFBF"},
                    },
                    "cell_shading": {"color": "DDEBF7"},
                    "alignment": "center",
                },
            },
        },
        "document_setup": {
            "orientation": "portrait",
            "margins_cm": {"top": 2.5, "bottom": 2.5, "left": 2.5, "right": 2.5},
            "paper_size": "A4",
        },
        "sections": [
            {
                "header": {"left": "{title}", "center": "Confidentiel", "right": "{date:%d/%m/%Y}"},
                "footer": {"left": "{author}", "center": "{filename}", "right": "Page {page}/{total_pages}"},
                "blocks": [
                    {"type": "heading", "text": "Contrat de prestation", "level": 1, "style": "title"},
                    {"type": "paragraph", "text": "Entre les soussignés :", "style": "body"},
                    {
                        "type": "table",
                        "rows": [
                            ["Client", "Apollia"],
                            ["Référence", "REF-2026-05-18"],
                            ["Montant", "10 000 € HT"],
                        ],
                        "header_row": True,
                        "style": "header_table",
                        "column_widths_cm": [5, 10],
                    },
                    {"type": "page_break"},
                    {"type": "heading", "text": "Article 1 — Objet", "level": 2},
                    {"type": "paragraph", "text": "Le présent contrat a pour objet…", "style": "body"},
                    {
                        "type": "list",
                        "list_type": "bullet",
                        "items": ["Item 1", "Item 2", "Item 3"],
                    },
                ],
            }
        ],
        "overwrite": True,
    },
)
```

### Ajouter une annexe

```python
await ctx.a2a_invoke(
    "docx.append-section",
    {
        "path": "/sandbox/contrat.docx",
        "section_break_before": True,
        "new_section_setup": {"orientation": "landscape"},
        "blocks": [
            {"type": "heading", "text": "Annexe A — Détails techniques", "level": 1},
            {"type": "paragraph", "text": "Description..."},
        ],
    },
)
```

### Extraire les tables seulement

```python
result = await ctx.a2a_invoke(
    "docx.extract-tables",
    {"path": "/data/rapport.docx"},
)
data = json.loads(result["result"]["text"])
# data["tables"] → [{"index": 0, "rows": [["...", "..."], ...], "rows_count": 5, "cols_count": 3}, ...]
```

### Rendre un template Jinja2

Template `template.docx` (créé dans Word) avec :

> Cher {{ client.nom }},
>
> Votre commande N° {{ commande.id }} du {{ commande.date }} pour un montant de **{{ commande.total }} €** vient d'être confirmée.
>
> Détail :
> {% for item in commande.items %}- {{ item.label }} : {{ item.prix }} €
> {% endfor %}
>
> {% if signature %}Signé électroniquement le {{ signature.date }}.{% endif %}

```python
await ctx.a2a_invoke(
    "docx.render-from-template",
    {
        "template_path": "/templates/confirmation-commande.docx",
        "output_path": "/sandbox/confirmation-12345.docx",
        "context": {
            "client": {"nom": "Dupont"},
            "commande": {
                "id": "12345",
                "date": "2026-05-18",
                "total": 10000,
                "items": [
                    {"label": "Mission 1", "prix": 5000},
                    {"label": "Mission 2", "prix": 5000},
                ],
            },
            "signature": {"date": "2026-05-18T14:30"},
        },
        "strict_undefined": True,
        "overwrite": True,
    },
)
```

Mode `strict_undefined: true` (défaut) : si une variable Jinja2 est utilisée dans le template mais absente du `context`, le worker échoue avec `UNDEFINED_VARIABLES`. Mode `false` : les variables manquantes sont laissées en place dans le doc rendu.

### Depuis Chat Libre

Les skills deviennent `a2a:docx.read`, `a2a:docx.write`, etc.

### Eval

```bash
python3 docx-worker/eval/run-eval.py
```

22 cas couvrant les 5 skills + erreurs typées + placeholders + styles avancés.

## Headers/footers — 10 placeholders disponibles

Dans chaque zone `left` / `center` / `right` d'un `header` ou `footer` :

| Placeholder | Description | Dynamique ? |
|---|---|---|
| `{page}` | Numéro de page courante | ✅ Champ Word PAGE |
| `{total_pages}` | Nombre total de pages | ✅ Champ Word NUMPAGES |
| `{date}` ou `{date:%fmt}` | Date du jour (défaut ISO, ex: `{date:%d/%m/%Y}` → `18/05/2026`) | Au moment du write |
| `{time}` ou `{time:%fmt}` | Heure (défaut `HH:MM`) | Au moment du write |
| `{datetime}` ou `{datetime:%fmt}` | Datetime ISO (défaut, sans microsec) | Au moment du write |
| `{author}` | Auteur (core_properties) | Au moment du write |
| `{title}` | Titre (core_properties) | Au moment du write |
| `{subject}` | Sujet (core_properties) | Au moment du write |
| `{filename}` | Basename du `output_path` | Au moment du write |
| `{section_num}` | Index 1-based de la section | Au moment du write |

Placeholders inconnus : laissés tels quels (texte littéral).

## Référence — StyleSpec

```jsonc
{
  "font": {
    "name": "Calibri",
    "size_pt": 11,
    "bold": false,
    "italic": false,
    "underline": false,
    "strike": false,
    "color": "000000",
    "highlight": "yellow"  // yellow|green|cyan|magenta|blue|red|dark*|gray|black|white
  },
  "paragraph": {
    "alignment": "left|center|right|justify",
    "space_before_pt": 0,
    "space_after_pt": 6,
    "line_spacing": 1.15,
    "first_line_indent_cm": 0,
    "left_indent_cm": 0,
    "right_indent_cm": 0
  },
  "table": {
    "borders": {
      "top":    {"style": "single|double|thick|dashed|dotted|none", "size_pt": 0.5, "color": "000000"},
      "bottom": {"...": "..."},
      "left":   {"...": "..."},
      "right":  {"...": "..."},
      "inside_h": {"...": "..."},
      "inside_v": {"...": "..."}
    },
    "cell_shading": {"color": "DDEBF7", "pattern": "clear"},
    "alignment": "left|center|right",
    "autofit": true
  }
}
```

**Couleurs** : hex 6 chiffres `"RRGGBB"` ou 8 chiffres `"AARRGGBB"`. Préfixe `#` toléré. Insensible à la casse.

## Types de blocs supportés (write/append)

- **`paragraph`** : `{type, text? | runs?: [{text, font?}], style?, alignment?}`
- **`heading`** : `{type, text, level: 1-9, style?}`
- **`table`** : `{type, rows: [[Cell, ...]], style?, column_widths_cm?, header_row?, cell_styles?}` — Cell = string OU `{text, style?, alignment?, merge?: {cols}}`
- **`list`** : `{type, items: [str | nested], list_type: "bullet"|"number", style?}` — nesting simple (un niveau d'indentation textuelle en v0.1.0)
- **`image`** : `{type, path, width_cm?, height_cm?, alignment?}` — PNG/JPEG/GIF/TIFF/BMP
- **`page_break`** : `{type: "page_break"}`
- **`section_break`** : `{type: "section_break", break_type: "next_page|continuous|odd_page|even_page"}` — traité comme `page_break` en v0.1.0 (limitations docx.write)

## Configuration

Aucune. Tous les paramètres passent dans la payload.

## Limitations connues

- Formats : `.docx` uniquement. `.doc` (Word 97-2003), `.docm` (macros), `.rtf` → `UNSUPPORTED_FORMAT`.
- En `docx.read`, tous les blocs sont rattachés à la **première section** (mapping bloc → section non implémenté en v0.1.0 — requiert parsing XML w:sectPr).
- Listes en `write` : niveau simple uniquement (bullet ou number flat). Imbrication représentée par indentation textuelle.
- `section_break` block en `docx.write` : traité comme `page_break` (v0.1.0). Pour de vraies sections multiples, utiliser la liste `sections` au top-level.
- Multi-level numbered lists complexes : non supportés (extension future).
- Embedded objects (Excel, PowerPoint, OLE) : non supportés.
- Bookmarks, cross-references, table of contents, footnotes, comments, track changes : non exposés ni créables.
- Form fields, content controls : non supportés.
- Détection précise des merged cells en lecture : best-effort (texte aggregé par cellule).
- Streaming pour fichiers > 100 MB : non optimisé.

## Sécurité

- `dangerous_tools_allowed = false`
- `tools_required = []`
- Aucun accès réseau
- Jinja2 sandbox (docxtpl utilise SandboxedEnvironment) — pas d'exécution de code arbitraire
- Validations internes : extension `.docx` requise, taille ≤ 100 MB, refus `.doc`/`.docm`, `overwrite: false` par défaut, paths d'images validés (existence + extension whitelist)

## License

MIT © Apollia OS

Voir [`CHANGELOG.md`](./CHANGELOG.md) pour l'historique des versions.
