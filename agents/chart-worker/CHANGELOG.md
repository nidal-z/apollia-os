# Changelog — chart-worker

Toutes les modifications notables de ce worker sont documentées ici.

Format inspiré de [Keep a Changelog](https://keepachangelog.com/), versioning [SemVer](https://semver.org/).

## [0.1.0] — 2026-05-18

### Added
- Release initiale.
- Skill `chart.bar` : bar chart vertical/horizontal × grouped/stacked, value_labels optionnels. Accepte `data` en `[number]` (avec `categories`) ou `[{x, y}]`.
- Skill `chart.line` : line chart multi-series avec :
  - Détection auto du `x_type` (number / datetime ISO / category)
  - `line_style` (solid/dashed/dotted), `line_width`, `marker` (o/s/^/D/x), `color` per-series
  - `y_log_scale`, `area_fill` optionnels
  - Formatter datetime auto (`%Y-%m-%d`) avec rotation labels
- Skill `chart.pie` : pie / donut chart avec :
  - `hole_size` 0-0.9 pour donut
  - `explode` par slice (offset 0-0.3)
  - `start_angle`, `colors` personnalisables
  - `show_percent` + `show_values` combinables (affichage "X%\n(valeur)")
- Skill `chart.scatter` : scatter / bubble chart avec :
  - 6 marker styles (circle/square/triangle/diamond/x/plus)
  - `size` per-point pour bubble effect
  - **Régression linéaire optionnelle** : droite + R² annoté dans la légende
  - Axes datetime/number combinables
- Skill `chart.heatmap` : heatmap matrice avec :
  - 8 colormaps whitelistées (`viridis`, `plasma`, `magma`, `inferno`, `Blues`, `Reds`, `Greens`, `RdBu`)
  - `vmin`/`vmax` explicites, colorbar avec label optionnel
  - `show_values` overlay avec auto-contraste (texte blanc sur cellules sombres, noir sinon)
  - `value_format` personnalisable (Python format string)
- 3 thèmes intégrés : `default` (matplotlib classique), `dark` (fond `#0F1419`, palette adaptée), `minimal` (sans spines top/right, grille discrète, palette pastel)
- 2 modes de sortie : `file` (écrit sur disque + retourne path) et `base64` (retourne content_base64, pas d'écriture)
- Eval suite avec 22+ cas couvrant les 5 skills + erreurs typées + safety checks + datetime axes + régression linéaire.

### Notes
- Worker **déterministe pur** — n'utilise pas `ctx.llm` ni de boucle ReAct.
- Stateless — aucune mémoire entre appels.
- Headless rendering : `matplotlib.use("Agg")` appelé **au niveau module avant import pyplot**.
- Dépendance unique : `matplotlib==3.9.2` (avec numpy/pillow/etc. transitives via wheels). venv ~30 MB.
- Schemas via `TypedDict` stdlib uniquement.
- 3 palettes par défaut adaptées à chaque thème (8 couleurs auto-cyclées par series).

### Limitations connues
- Pas de **3D charts** (matplotlib en supporte mais peu utile)
- Pas de **subplots multiples** — 1 chart = 1 fichier (futur `chart-composite-worker`)
- Pas d'**annotations arbitraires** (flèches, callouts, boxes)
- Pas d'**animations**
- Pas de **chart interactif HTML** — futur `chart-interactive-worker` avec plotly + kaleido (écarté en v0.1.0 pour ~50 MB supplémentaires + binaire headless cross-platform)
- **Polices** : DejaVu Sans matplotlib default uniquement (pas de custom font loading)
- **Cap** : 1 000 000 data points total, dimensions `[1, 1000]` inches, DPI `[50, 600]`
- **Formats** : `.png` et `.svg` uniquement (pas de JPEG/PDF/EPS/PS)

## [0.1.1] — 2026-05-19

### Changed
- **Breaking** : remplacé le workaround champ `op` payload par la lecture de `task.skill_id` (propagé par le runtime A2A depuis 2026-05-19). Le worker dispatche désormais sur le full skill_id via `extract_skill_id(task)`.
- Nouveaux codes d'erreur : `MISSING_SKILL_ID`, `UNKNOWN_SKILL_ID`. Les codes `MISSING_FIELD`/`INVALID_TYPE` (anciennement renvoyés pour `op` manquant/invalide) ne sont plus émis pour ce vecteur.
- Eval cases : champ `skill_id` top-level ajouté à chaque case ; champ `op` retiré des payloads.

### Removed
- Convention `op` (bar|line|pie|scatter|heatmap) dans le payload — caller invoque maintenant `ctx.a2a_invoke("<skill>", {...})` sans champ `op`.
