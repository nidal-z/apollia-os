# Design System — Apollia Desktop

> Reference for visual tokens exposed by `crates/apollia-desktop/ui/src/app.css`.
> Gouverné par **ADR-077** (design tokens v2).
>
> Les valeurs concrètes vivent dans `app.css`; ce document est la fiche de
> lecture pour builders et designers. Ne jamais dupliquer une valeur ici —
> utiliser la source (`--shadow-elev-X`, `--surface-N`, etc.) ou les wrappers
> TypeScript exportés par `src/lib/design/tokens.ts`.

---

## Élévation — 5 niveaux + inset rim light

L'échelle d'élévation combine **ombres multi-layers** et **rim light interne**
pour conférer du relief (findings F.19, F.20). Le rim light est inséré par un
`inset 0 1px 0` — warm en light (blanc cassé `rgba(255, 252, 240, ...)`),
chaud en dark (`hsl(32 30% 70% / 0.12+)`) pour éviter l'effet "black on
black" (F.55).

| Token | Usage | Light | Dark |
|---|---|---|---|
| `--shadow-elev-0` | Surfaces inline, lignes de tableau | `0 1px 0 rgba(120,100,60,.04)` + rim `.5` | `0 1px 0 rgba(0,0,0,.20)` + rim `.10` |
| `--shadow-elev-1` | Boutons, cards au repos | 2-layers + rim `.6` | 2-layers + rim `.12` |
| `--shadow-elev-2` | Cards surélevées, popovers, menus | 2-layers + rim `.6` | 2-layers + rim `.14` |
| `--shadow-elev-3` | Sheets, hover des cards | 2-layers + rim `.7` | 2-layers + rim `.16` |
| `--shadow-elev-4` | Modals hero, spotlight | 2-layers + teinte primary + rim `.8` | 2-layers + teinte primary + rim `.18` |

**Tailwind :** `shadow-elev-0` … `shadow-elev-4`.
**TypeScript :** `tokens.elevation.elev0` … `tokens.elevation.elev4`.

### Primary-tinted shadows

Utiliser pour les CTAs portant l'identité visuelle (pas pour les cards).

| Token | Usage |
|---|---|
| `--shadow-primary-sm` | Boutons primaires au repos |
| `--shadow-primary-md` | CTAs surélevés (Wizard next, Chat send) |
| `--shadow-primary-lg` | Hover de CTA hero |
| `--shadow-primary-xl` | Big featured (Start agent, Send — combiné à `md`) |

---

## Surfaces

La palette surface encode trois niveaux de profondeur, + `--background` /
`--muted` / `--border` pour les zones inter-surfaces.

### Light (warm sand / parchment)

| Token | Valeur (HSL) | Usage |
|---|---|---|
| `--background` | `38 28% 90%` | Body |
| `--surface-1` | `40 35% 96%` | Card élevée, modals |
| `--surface-2` | `38 30% 92%` | Card resting, panels |
| `--surface-3` | `36 22% 86%` | Recessed / inset (empty states, diffs) |
| `--muted` | `36 20% 85%` | Backgrounds neutres |
| `--border` | `36 16% 80%` | Bordures |

### Dark (warm charcoal — F.33, F.34)

Décalée warm : target `hsl(28 8% 9%)` au lieu de neutre `240 10% 10%`. Tous
les niveaux conservent la même teinte `28°` pour préserver la cohérence.

| Token | Valeur (HSL) | Usage |
|---|---|---|
| `--background` | `28 8% 9%` | Body — brun cuir, pas neutre |
| `--surface-1` | `28 10% 14%` | Card élevée |
| `--surface-2` | `28 9% 12%` | Card resting |
| `--surface-3` | `28 8% 10%` | Recessed |
| `--muted` | `28 7% 18%` | Neutral fills |
| `--border` | `28 8% 22%` | Bordures visibles |

**Tailwind :** `bg-surface-1` / `bg-surface-2` / `bg-surface-3`.

---

## Glass borders

Les bordures glass sont différenciées light vs dark (F.74/F.75) avec une
variante hover pour souligner l'interactivité.

| Token | Usage |
|---|---|
| `--glass-border-light` | Bordure card light mode au repos |
| `--glass-border-light-hover` | Idem au hover |
| `--glass-border-dark` | Bordure card dark mode au repos |
| `--glass-border-dark-hover` | Idem au hover |

Les classes `.glass-border` et `.glass-border-subtle` basculent
automatiquement selon le mode.

### Inset — dark fix (F.55)

`.glass-inset` utilise désormais `hsl(32 40% 78% / 0.12)` en dark (warm
high-light 12 %) pour rester visible, au lieu de `rgba(255, 255, 255, 0.05)`
qui disparaissait.

---

## Primary utilities

Trois expositions canoniques du primary, utilisées sur les CTAs clés
(Dashboard empty state, Chat send, Agent start, Wizard next, Trigger fire —
F.41, A.1.10) :

| Classe | Rendu |
|---|---|
| `.bg-primary-solid` | Couleur pleine + `shadow-primary-sm` au repos, `md` au hover |
| `.bg-primary-gradient` | Gradient 135° primary→secondary + `shadow-primary-md`, `lg` au hover |
| `.border-primary-subtle` | Bordure `hsl(var(--primary) / .25)`, `.50` au hover |

Exposés aussi comme variants du composant `<Button>` :
`variant="primary-solid"`, `variant="primary-gradient"`.

---

## Backdrop (F.56)

Modal / sheet backdrops utilisent une teinte différente selon le mode.

| Token | Light | Dark |
|---|---|---|
| `--backdrop` | `rgba(0,0,0,.40)` | `hsl(28 10% 5% / .70)` |
| `--backdrop-subtle` | `rgba(0,0,0,.25)` | `hsl(28 10% 5% / .50)` |

Classes : `.app-backdrop` (`backdrop-blur-md`) et `.app-backdrop-subtle`
(`backdrop-blur-sm`).

---

## Comment ajouter un nouveau token

1. Déclarer la variable dans `:root` (light) **et** `.dark` (dark) dans
   `app.css`.
2. Exposer via `src/lib/design/tokens.ts` si du code TypeScript doit y
   référer.
3. Si besoin d'une classe Tailwind, étendre `tailwind.config.ts`
   (`boxShadow`, `colors`, `backgroundImage`).
4. Documenter ici le **pourquoi** et l'usage prévu — jamais les valeurs.

## Règles

- **Jamais** de `box-shadow` hardcodé dans un `.svelte` — toujours `var(...)`.
- **Jamais** de couleur dark fixée à `#0f…` / `#1f…` — utiliser `--background` / `--surface-*`.
- Les tokens visibles (`.bg-primary-gradient` par ex.) ne sont utilisés que
  pour l'identité de marque. Les surfaces neutres passent par `bg-surface-N`
  ou `bg-background`.

---

**Route de prévisualisation (dev only) :** `#design` — montre les 5
élévations, les palettes light/dark côte à côte, et les primary utilities.
Accessible seulement en `import.meta.env.DEV`.
