# xlsx-worker

> Manipulation de fichiers Excel (.xlsx) : lire, écrire, ajouter des lignes, patcher des cellules ciblées — préserve types, formules et merged cells. Styles avancés et lecture pandas.

Worker Apollia OS standalone. Cinq skills A2A déterministes, invocables par n'importe quel agent (custom director, Chat Libre via `a2a:*`, autres workers chaînés).

Décharge le LLM director de la génération de code openpyxl/pandas à la volée : la complexité (formules, dtypes, styles, conditional formatting) est encapsulée derrière une API JSON typée.

## Skills exposés

| Skill ID | Description | Input principal |
|---|---|---|
| `xlsx.read` | Lit le workbook (cellules brutes, types, merged ranges, freeze panes). | `path` |
| `xlsx.read-as-dataframe` | Lit en pandas DataFrame (dtype inference, header detection, skip/nrows). | `path` |
| `xlsx.write` | Crée un .xlsx multi-sheet avec styles avancés et conditional formatting. | `output_path`, `sheets` |
| `xlsx.append-rows` | Ajoute des lignes sans toucher au reste. | `path`, `rows` |
| `xlsx.update-cells` | Patch ciblé par A1 notation. | `path`, `sheet_name`, `updates` |

**Format supporté** : `.xlsx` uniquement. `.xls`, `.xlsm`, `.xlsb` retournent `UNSUPPORTED_FORMAT`.

## Dispatch multi-skills

Depuis Apollia v0.1.0 (2026-05-19), le runtime propage le `skill_id` invoqué dans `AIPTask.skill_id`. Le worker dispatche directement sur le full skill_id via `apollia.utils.a2a.extract_skill_id(task)`. Côté caller : aucune convention spéciale dans le payload — invoque simplement `ctx.a2a_invoke("<skill_id>", {...})`.


## Installation

```bash
apollia agent install ./xlsx-worker
```

Au premier passage à `Active`, Apollia crée le venv et installe :
- `openpyxl==3.1.5` (lecture/écriture .xlsx + styles)
- `pandas==2.2.3` (read-as-dataframe — tire numpy et python-dateutil)

Total venv : ~50 MB.

## Usage

### Lire

```python
import json

result = await ctx.a2a_invoke(
    "xlsx.read",
    {"path": "/data/budget.xlsx"},
    timeout_secs=60,
)
data = json.loads(result["result"]["text"])
# data["sheets"][0]["rows"] → [[v, v, v], ...]
# data["sheets"][0]["merged_ranges"] → ["A1:B2", ...]

# Mode formules
result = await ctx.a2a_invoke("xlsx.read", {
    "path": "/data/budget.xlsx",
    "read_mode": "formulas",
    "include_types": True,
})
# data["sheets"][0]["rows"][0] → [{"value": "=SUM(...)", "type": "formula"}, ...]
```

### Lire en DataFrame (pandas)

```python
result = await ctx.a2a_invoke(
    "xlsx.read-as-dataframe",
    {
        "path": "/data/ventes.xlsx",
        "sheet_name": "2026",
        "header_row": 0,
        "dtypes_hint": {"montant": "float", "date": "datetime"},
        "nrows": 1000,
    },
)
data = json.loads(result["result"]["text"])
# data["records"] → [{"date": "2026-05-18T00:00:00.000Z", "produit": "X", "montant": 1234.5}, ...]
# data["dtypes"] → {"date": "datetime64[ns]", "produit": "string", "montant": "float64"}
```

### Écrire avec styles avancés

```python
await ctx.a2a_invoke(
    "xlsx.write",
    {
        "output_path": "/sandbox/rapport.xlsx",
        "named_styles": {
            "header": {
                "font": {"bold": True, "color": "FFFFFF", "size": 12},
                "fill": {"color": "1F4E78"},
                "alignment": {"horizontal": "center", "vertical": "center"},
                "border": {
                    "bottom": {"style": "medium", "color": "000000"},
                },
            },
            "currency": {
                "number_format": "#,##0.00 €",
                "alignment": {"horizontal": "right"},
            },
            "highlight_red": {
                "fill": {"color": "FFC7CE"},
                "font": {"color": "9C0006", "bold": True},
            },
        },
        "sheets": [
            {
                "name": "Ventes",
                "headers": ["Date", "Produit", "Montant"],
                "rows": [
                    ["2026-05-18", "Produit A", 1234.50],
                    ["2026-05-19", "Produit B", 999.00],
                    ["2026-05-20", "Produit C", 5500.00],
                ],
                "column_widths": {"A": 14, "B": 25, "C": 14},
                "row_heights": {"1": 28},
                "freeze": {"row": 1, "col": 0},
                "auto_filter": True,
                "styles_apply": [
                    {"range": "A1:C1", "style": "header"},
                    {"range": "C2:C1000", "style": "currency"},
                ],
                "conditional_formatting": [
                    {"range": "C2:C1000", "rule": "greater_than", "value": 5000, "style": "highlight_red"},
                ],
            }
        ],
        "overwrite": True,
    },
)
```

### Ajouter des lignes au quotidien

```python
await ctx.a2a_invoke(
    "xlsx.append-rows",
    {
        "path": "/sandbox/rapport.xlsx",
        "sheet_name": "Ventes",
        "rows": [["2026-05-21", "Produit D", 777.77]],
    },
)
```

### Patcher des cellules ciblées

```python
await ctx.a2a_invoke(
    "xlsx.update-cells",
    {
        "path": "/sandbox/rapport.xlsx",
        "sheet_name": "Ventes",
        "updates": [
            {"cell_ref": "C100", "value": "=SUM(C2:C99)"},
            {"cell_ref": "B100", "value": "TOTAL"},
        ],
    },
)
```

### Depuis Chat Libre

Les skills deviennent des outils virtuels `a2a:xlsx.read`, `a2a:xlsx.write`, etc. directement utilisables par le LLM.

### Eval

```bash
python3 xlsx-worker/eval/run-eval.py
```

Le runner crée des fixtures .xlsx temporaires, exécute 17 cas et reporte le pass rate.

## Référence — StyleSpec

Tous les champs sont optionnels (omettre = défaut Excel).

```jsonc
{
  "font": {
    "name": "Calibri",
    "size": 11,
    "bold": false,
    "italic": false,
    "underline": false,
    "strike": false,
    "color": "000000"
  },
  "fill": {
    "color": "FFFFFF",
    "pattern": "solid"
  },
  "border": {
    "top":    {"style": "thin",  "color": "000000"},
    "bottom": {"style": "thick", "color": "000000"},
    "left":   {"style": "thin",  "color": "000000"},
    "right":  {"style": "thin",  "color": "000000"}
  },
  "alignment": {
    "horizontal": "general | left | center | right | justify",
    "vertical":   "top | center | bottom",
    "wrap_text":  false,
    "indent":     0
  },
  "number_format": "General | @ | 0 | 0.00 | #,##0 | #,##0.00 | 0% | yyyy-mm-dd | <free-form Excel>"
}
```

**Styles de bordure** : `thin`, `medium`, `thick`, `dashed`, `dotted`, `double`.

**Couleurs** : hex 6 chiffres `"RRGGBB"` (recommandé, alpha forcé à `FF`) ou 8 chiffres `"AARRGGBB"`. Préfixe `#` toléré. Insensible à la casse.

## Référence — Conditional formatting

```jsonc
{
  "range": "C2:C1000",
  "rule": "greater_than | less_than | equal_to | not_equal_to | between | not_between | contains_text | not_contains_text | begins_with | ends_with",
  "value": 100,             // pour rules unaires
  "values": [10, 50],       // pour between/not_between
  "style": "highlight"      // référence à un style dans named_styles
}
```

**Limites v0.1.0** : pas de gradients, pas de databars/icon sets, pas de règles à formule libre — extensions futures dans un `xlsx-cf-advanced-worker`.

## Configuration

Aucune. Tous les paramètres passent dans la payload.

## Limitations connues

- Format `.xls` ancien (Excel 97-2003), `.xlsm` (macros VBA), `.xlsb` (binaire) : non supportés.
- Charts, pivot tables, images, formes, commentaires, data validation : conservés à l'ouverture mais ni exposés en `read` ni créables en `write` en v0.1.0.
- Merged cells : signalées en `read`, **pas créables en `write`** en v0.1.0.
- Fichiers chiffrés (password-protected) : non supportés (`PARSE_ERROR`).
- Taille max en lecture : 100 MB (configurable plus tard si demande).
- Streaming pour très gros fichiers : non optimisé.
- En mode `read_mode="values"`, les formules retournent leur valeur cachée — si le fichier n'a jamais été ouvert dans Excel, cette valeur peut être `None`. Utiliser `read_mode="formulas"` pour récupérer le texte de la formule.
- Dates : openpyxl renvoie `datetime` natifs sérialisés en ISO 8601. En écriture, passer des chaînes ISO les stocke en strings — pour avoir un type date, appliquer un `number_format` "yyyy-mm-dd" via `styles_apply`.

## Sécurité

- `dangerous_tools_allowed = false`
- `tools_required = []` (openpyxl/pandas accèdent au filesystem directement, dans le sandbox de venv)
- Aucun accès réseau
- Validations internes : extension `.xlsx` requise, taille fichier ≤ 100 MB, cap `max_rows × max_cols` en read, `overwrite: false` par défaut sur write

## License

MIT © Apollia OS

Voir [`CHANGELOG.md`](./CHANGELOG.md) pour l'historique des versions.
