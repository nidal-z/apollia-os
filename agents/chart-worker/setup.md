# Setup — chart-worker v0.1.0

## Prérequis

- Apollia OS **v0.1.0+** installé et fonctionnel (`apollia --version`)
- Python 3.13 (bundled avec Apollia)

### Dépendances Python (installées automatiquement)

| Package | Version | Rôle |
|---|---|---|
| `matplotlib` | `3.9.2` | Moteur de rendu (PNG/SVG) |

`matplotlib` tire automatiquement : `numpy==2.x`, `pillow==11.x`, `kiwisolver`, `contourpy`, `cycler`, `fonttools`, `pyparsing`, `python-dateutil`, `packaging`. **Taille totale du venv : ~30 MB.**

### Dépendances natives système

Aucune. `numpy`, `pillow`, `contourpy` et `kiwisolver` fournissent des wheels précompilées pour macOS (arm64/x86_64), Linux (x86_64/aarch64), Windows.

## Installation

```bash
apollia agent install ./chart-worker
```

Cette commande :
1. Valide `agent.toml` et le code Python (duck typing).
2. Copie vers `~/.apollia/agents/packages/chart-worker/`.
3. Enregistre dans `~/.apollia/agents.db`.

Au premier passage à `Active` :

```
~/.apollia/venvs/chart-worker/venv/
  └─ matplotlib==3.9.2 (+ numpy, pillow, kiwisolver, contourpy, ...)
```

Installation : 5-15 secondes selon connexion.

## Vérification

```bash
apollia agent list | grep chart-worker
```

État attendu : `Active`.

Test rapide via eval :

```bash
python3 ~/.apollia/agents/packages/chart-worker/eval/run-eval.py
```

Sortie attendue : 22/22 cases passed (100%).

## Désinstallation

```bash
apollia agent uninstall chart-worker
rm -rf ~/.apollia/venvs/chart-worker
```

## Troubleshooting

### Le worker reste `Initializing` longtemps

Pip télécharge matplotlib + numpy + pillow. Surveiller :

```bash
tail -f ~/.apollia/logs/runtime.log | grep chart-worker
```

Si bloqué > 2 min : connectivité PyPI, espace disque (`df -h ~/.apollia/`).

### `EXECUTION_FAILED: _tkinter.TclError` ou `RuntimeError: main thread`

Le backend GUI est tenté au lieu de Agg. Ce worker appelle `matplotlib.use("Agg")` **avant** d'importer pyplot — si cette erreur survient, c'est qu'un autre module a déjà initialisé un backend. Réinstaller le venv :

```bash
apollia agent uninstall chart-worker
rm -rf ~/.apollia/venvs/chart-worker
apollia agent install ./chart-worker
```

### `MISSING_SKILL_ID`

Le runtime n'a pas propagé de `skill_id` jusqu'au worker. Le worker chart-worker est multi-skills : il a besoin du `skill_id` pour dispatcher. Invoquer via `ctx.a2a_invoke("<skill>", payload)` pour que le runtime propage automatiquement.

### `UNKNOWN_SKILL_ID`

Le `skill_id` reçu n'est pas dans la liste : `"chart.bar", "chart.line", "chart.pie", "chart.scatter", "chart.heatmap"`. Vérifier l'orthographe côté caller.

### `INVALID_FORMAT`

Extension non whitelistée. Seuls `.png` et `.svg` sont acceptés. Pour JPEG/PDF/EPS/PS : non supportés en v0.1.0.

### `INVALID_DATA: NaN ou Inf`

matplotlib produit des outputs vides ou crashe avec NaN/Inf. Filtrer/nettoyer les données côté caller avant l'appel.

### `TOO_MANY_POINTS`

Total data points > 1 000 000. Options :
- Réduire l'échantillonnage côté caller (downsampling)
- Utiliser un heatmap (1 cellule par point) au lieu d'un scatter à 1M points

### `INVALID_COLORMAP`

Colormap hors whitelist. Valeurs acceptées : `viridis`, `plasma`, `magma`, `inferno`, `Blues`, `Reds`, `Greens`, `RdBu`.

### `INVALID_STYLE: Couleur hex invalide`

Format attendu : `"RRGGBB"` ou `"AARRGGBB"`. Préfixe `#` toléré. Pas de noms de couleurs.

### `UNSUPPORTED_X_TYPE`

Datetime parsing impossible. Format attendu : ISO 8601 :
- `"2026-05-18"` (date seule)
- `"2026-05-18T14:30:00"` (datetime sans tz)
- `"2026-05-18T14:30:00Z"` (UTC)
- `"2026-05-18T14:30:00+02:00"` (avec offset)

### Charts illisibles (texte/labels coupés)

`width_inches` ou `height_inches` trop petits, ou nombre de labels trop élevé. Solutions :
- Augmenter `width_inches` / `height_inches`
- Rotation auto des labels datetime déjà appliquée
- Pour heatmap avec beaucoup de labels : rotation 45° auto sur col_labels

### Polices manquantes

Le worker utilise DejaVu Sans (bundled matplotlib). Si vous voyez des carrés à la place de caractères Unicode (CJK, arabe, etc.), c'est attendu en v0.1.0 — pas de custom font loading.

### Logs détaillés

```bash
RUST_LOG=apollia=debug apollia-os start --foreground 2>&1 | grep chart-worker
```
