# chart-worker

> Génération de charts (PNG/SVG) depuis JSON : bar, line, pie, scatter, heatmap. Styles avancés (3 thèmes), régression linéaire, mode file ou base64.

Worker Apollia OS standalone. Cinq skills A2A déterministes invocables par n'importe quel agent (custom director, Chat Libre via `a2a:*`, autres workers chaînés).

Décharge le LLM director : plus besoin de générer du code matplotlib à la volée — fournis `{op, data, title, theme, ...}`, reçois un path PNG/SVG ou du base64.

## Skills exposés

| Skill ID | Description | Input principal |
|---|---|---|
| `chart.bar` | Bar chart vertical/horizontal, grouped/stacked. | `series` |
| `chart.line` | Line chart datetime/number/category, markers, line_style. | `series` |
| `chart.pie` | Pie / donut, explode, show_percent. | `data` |
| `chart.scatter` | Scatter / bubble, régression linéaire optionnelle. | `series` |
| `chart.heatmap` | Heatmap matrice, 8 colormaps, show_values. | `matrix` |

**Formats de sortie** : `.png` (raster, DPI configurable) ou `.svg` (vector).

## Dispatch multi-skills

Depuis Apollia v0.1.0 (2026-05-19), le runtime propage le `skill_id` invoqué dans `AIPTask.skill_id`. Le worker dispatche directement sur le full skill_id via `apollia.utils.a2a.extract_skill_id(task)`. Côté caller : aucune convention spéciale dans le payload — invoque simplement `ctx.a2a_invoke("<skill_id>", {...})`.


## Installation

```bash
apollia agent install ./chart-worker
```

Au premier passage à `Active`, Apollia crée le venv et installe :
- `matplotlib==3.9.2` (avec numpy, pillow, kiwisolver, contourpy, fonttools, cycler, pyparsing, python-dateutil, packaging tirés transitivement)

Total venv : ~30 MB. Aucune dépendance native système (wheels précompilées).

## Champs communs (présents dans les 5 skills)

| Champ | Type | Défaut | Description |
|---|---|---|---|
| `output_path` | string | (requis si mode=file) | Chemin `.png` ou `.svg` |
| `output_mode` | string | `"file"` | `"file"` ou `"base64"` |
| `format` | string | inféré | `"png"` ou `"svg"` |
| `title` | string | — | Titre du chart |
| `xlabel`, `ylabel` | string | — | Labels d'axes |
| `width_inches` | float | `10.0` | Largeur, 1-1000 |
| `height_inches` | float | `6.0` | Hauteur, 1-1000 |
| `dpi` | integer | `150` | 50-600, PNG seulement |
| `theme` | string | `"default"` | `"default"`, `"dark"`, `"minimal"` |
| `legend` | boolean | `true` | Affiche la légende |
| `grid` | boolean | `true` | Affiche la grille |
| `overwrite` | boolean | `false` | Mode file uniquement |
| `font_size` | integer | `11` | Taille police générale |
| `title_font_size` | integer | `14` | Taille police titre |

En mode `output_mode="base64"`, `output_path` et `overwrite` sont ignorés silencieusement.

## Usage

### Bar chart simple

```python
import json

await ctx.a2a_invoke(
    "chart.bar",
    {
        "output_path": "/sandbox/ventes.png",
        "title": "Ventes 2026 par trimestre",
        "categories": ["Q1", "Q2", "Q3", "Q4"],
        "series": [
            {"name": "Produit A", "data": [120, 150, 180, 210], "color": "1F4E78"},
            {"name": "Produit B", "data": [80, 95, 110, 130], "color": "70AD47"},
        ],
        "bar_style": "grouped",
        "value_labels": True,
        "theme": "minimal",
    },
)
```

### Line chart avec datetime

```python
await ctx.a2a_invoke(
    "chart.line",
    {
        "output_path": "/sandbox/timeline.svg",
        "title": "Trafic web 2026",
        "xlabel": "Date", "ylabel": "Visiteurs",
        "x_type": "datetime",
        "series": [
            {
                "name": "Visiteurs uniques",
                "data": [
                    {"x": "2026-01-01", "y": 1200},
                    {"x": "2026-02-01", "y": 1450},
                    {"x": "2026-03-01", "y": 1820},
                ],
                "marker": "o",
                "line_style": "solid",
            }
        ],
    },
)
```

### Donut

```python
await ctx.a2a_invoke(
    "chart.pie",
    {
        "output_path": "/sandbox/parts.png",
        "title": "Répartition CA",
        "data": [
            {"label": "Conseil", "value": 45},
            {"label": "Formation", "value": 30},
            {"label": "Support", "value": 25},
        ],
        "donut": True,
        "hole_size": 0.4,
        "explode": [0.05, 0, 0],
    },
)
```

### Scatter avec régression linéaire

```python
await ctx.a2a_invoke(
    "chart.scatter",
    {
        "output_path": "/sandbox/correl.png",
        "title": "Prix vs surface",
        "xlabel": "Surface (m²)", "ylabel": "Prix (k€)",
        "series": [{
            "name": "Annonces",
            "data": [
                {"x": 50, "y": 250}, {"x": 75, "y": 380},
                {"x": 100, "y": 500}, {"x": 120, "y": 620},
            ],
        }],
        "regression": True,
    },
)
# Le R² est annoté dans la légende : "Annonces (R²=0.998)"
```

### Heatmap

```python
await ctx.a2a_invoke(
    "chart.heatmap",
    {
        "output_path": "/sandbox/correl-matrix.png",
        "title": "Corrélations",
        "matrix": [
            [1.00, 0.85, -0.32],
            [0.85, 1.00, -0.15],
            [-0.32, -0.15, 1.00],
        ],
        "row_labels": ["A", "B", "C"],
        "col_labels": ["A", "B", "C"],
        "colormap": "RdBu",
        "vmin": -1, "vmax": 1,
        "show_values": True,
    },
)
```

### Mode base64 (pas d'écriture disque)

```python
result = await ctx.a2a_invoke(
    "chart.bar",
    {
        "output_mode": "base64",
        "format": "png",
        "categories": ["A", "B"],
        "series": [{"name": "v", "data": [1, 2]}],
    },
)
b64 = json.loads(result["result"]["text"])["content_base64"]
# embed in HTML : <img src="data:image/png;base64,{b64}" />
```

### Depuis Chat Libre

Les skills deviennent `a2a:chart.bar`, `a2a:chart.line`, etc.

### Eval

```bash
python3 chart-worker/eval/run-eval.py
```

22+ cas couvrant les 5 skills + erreurs typées + mode base64 + datetime axes + régression.

## Thèmes intégrés

| Thème | Description |
|---|---|
| `default` | matplotlib classique (fond blanc, palette saturée) |
| `dark` | Fond `#0F1419`, texte clair, palette adaptée aux dashboards sombres |
| `minimal` | Spines top/right retirées, grille discrète, palette pastel |

## Colormaps supportées (heatmap)

8 colormaps whitelistées :
- **Séquentielles** : `viridis` (défaut), `plasma`, `magma`, `inferno`, `Blues`, `Reds`, `Greens`
- **Divergente** : `RdBu` (idéale pour corrélations −1 à +1)

## Couleurs

Hex `RRGGBB` ou `AARRGGBB` (6 ou 8 chars). Préfixe `#` toléré. Insensible à la casse.
En `AARRGGBB`, l'alpha est ignoré (matplotlib utilise `#RRGGBB`).

## Configuration

Aucune. Tous les paramètres passent dans la payload.

## Limitations connues

- Pas de **3D charts** (matplotlib en supporte mais peu utile en pratique)
- Pas de **subplots multiples** (1 chart = 1 fichier — futur `chart-composite-worker`)
- Pas d'**annotations arbitraires** (flèches, callouts pointant un point spécifique)
- Pas d'**animations**
- Pas de **chart interactif HTML** — futur `chart-interactive-worker` avec plotly+kaleido
- **Polices** : DejaVu Sans (matplotlib default) uniquement. Pas de custom font loading en v0.1.0.
- **Cap** : 1 000 000 data points total par chart. Au-delà : `TOO_MANY_POINTS`.
- **Dimensions** : `width_inches` et `height_inches` dans `[1, 1000]`.
- **DPI** : `[50, 600]`.
- **Pie** : valeurs négatives refusées (`INVALID_DATA`).
- **Datetime** : ISO 8601 (`2026-05-18` ou `2026-05-18T14:30:00Z`).

## Codes d'erreur

| Code | Cause |
|---|---|
| `INVALID_PAYLOAD` | payload A2A vide |
| `MISSING_FIELD` | champ requis absent |
| `INVALID_TYPE` | mauvais type ou valeur non whitelistée |
| `INVALID_DATA` | NaN/Inf, lengths mismatch, value négative pour pie, dimensions hors plage |
| `INVALID_FORMAT` | extension output ≠ `.png`/`.svg` |
| `INVALID_COLORMAP` | colormap hors whitelist |
| `INVALID_STYLE` | hex color invalide, theme inconnu |
| `UNSUPPORTED_X_TYPE` | datetime parsing impossible |
| `TOO_MANY_POINTS` | > 1M data points total |
| `OUTPUT_EXISTS` | file existe + overwrite false |
| `EXECUTION_FAILED` | erreur matplotlib inattendue |

## Sécurité

- `dangerous_tools_allowed = false`
- `tools_required = []`
- Aucun accès réseau
- Rendering headless (`matplotlib.use("Agg")`) — pas de GUI/X11
- Validations internes : extension whitelist, cap data points, hex colors, colormap whitelist, dimensions bornées
- Pas d'exécution de code matplotlib arbitraire (schéma JSON ciblé par type)

## License

MIT © Apollia OS

Voir [`CHANGELOG.md`](./CHANGELOG.md) pour l'historique des versions.
