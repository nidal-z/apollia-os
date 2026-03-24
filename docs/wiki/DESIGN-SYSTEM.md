# Apollia OS — Design System

> **Version** : 1.0
> **Derniere mise a jour** : 2026-03-22
> **Stack** : Tauri v2 + Svelte 5 (runes) + Tailwind 3.4 + lucide-svelte + bits-ui + svelte-i18n
> **Repertoire UI** : `crates/apollia-desktop/ui/src/`

Ce document est la reference unique pour toute modification UI/UX dans Apollia OS Desktop.
Tout agent IA ou developpeur doit le lire avant de modifier un composant.

---

## 1. Principes directeurs

### 1.1 Identite visuelle : "Warm Glass"

L'identite Apollia combine trois influences :

| Influence | Ce qu'on retient |
|---|---|
| **Claude.ai** (Anthropic) | Fond creme doux, coins arrondis, badges pastel, ombres quasi invisibles |
| **Apple (macOS)** | Glassmorphism natif neutre, hierarchie par la lumiere, typo legere, micro-animations 200ms |
| **Apollia brand** | Accent bleu primaire (240° 91% 58%), gradient subtil brand dans le glass |

### 1.2 Regles fondamentales

1. **Hierarchie par la lumiere** — La profondeur est creee par le blur et les ombres, pas par la couleur
2. **Parcimonie de l'accent** — Le bleu primaire est reserve aux CTA, liens, focus rings et badges actifs. Jamais en fond, jamais en ombre
3. **Ombres neutres** — Les ombres sont toujours en `rgba(0,0,0,...)` ou teintes chaudes. Le brand glow subtil est reserve aux glass layers (app.css)
4. **Typography legere** — Maximum `font-semibold` (600) pour les titres de page. `font-bold` (700) est interdit
5. **Dark mode citoyen de premiere classe** — Pas un afterthought. Chaque token a une variante dark explicite
6. **Accessibilite integree** — Focus rings, aria labels, navigation clavier, WCAG AA sur chaque composant
7. **i18n obligatoire** — Toute string visible par l'utilisateur doit etre dans `en.json` + `fr.json`

---

## 2. Palette & Tokens

### 2.1 Variables CSS HSL (`app.css`)

#### Mode Light (`:root`)

| Token | Valeur HSL | Hex approx | Usage |
|---|---|---|---|
| `--primary` | `240 91% 58%` | `#3435f5` | Accent, CTA, liens, focus |
| `--primary-foreground` | `0 0% 100%` | `#ffffff` | Texte sur primary |
| `--secondary` | `260 60% 61%` | `#7c5fd6` | Accent secondaire (rare) |
| `--background` | `38 28% 90%` | `#e8e1d1` | Fond de page (creme chaud) |
| `--foreground` | `230 15% 14%` | `#1f2029` | Texte principal |
| `--card` | `40 35% 96%` | `#faf6ec` | Fond des cartes |
| `--muted` | `36 20% 85%` | `#ddd7cb` | Fond desature (tags, headers) |
| `--muted-foreground` | `220 10% 40%` | `#5c6370` | Texte secondaire |
| `--border` | `36 16% 80%` | `#d1cbc0` | Bordures |
| `--input` | `36 14% 84%` | `#dad5cb` | Fond inputs |
| `--ring` | `240 91% 58%` | `#3435f5` | Focus ring |
| `--destructive` | `0 72% 51%` | `#e04040` | Erreur, suppression |
| `--success` | `152 56% 42%` | `#2fa87a` | Succes |
| `--warning` | `38 92% 50%` | `#f5a800` | Avertissement |
| `--info` | `213 94% 47%` | `#0572ea` | Information |

#### Mode Dark (`.dark`)

| Token | Valeur HSL | Usage |
|---|---|---|
| `--background` | `240 12% 9%` | Charbon chaud (pas bleu pur) |
| `--foreground` | `0 0% 93%` | Texte off-white |
| `--card` | `240 10% 12%` | Fond cartes dark |
| `--muted` | `240 8% 16%` | Fond desature dark |
| `--muted-foreground` | `240 5% 58%` | Texte secondaire dark |
| `--border` | `240 6% 20%` | Bordures dark |
| `--primary` | `240 91% 62%` | Accent plus lumineux pour contraste |

### 2.2 Aliases resolus (var())

Disponibles dans `:root` pour usage inline :

```css
--apollia-bg        → hsl(var(--background))
--apollia-surface   → hsl(var(--card))
--apollia-border    → hsl(var(--border))
--apollia-text      → hsl(var(--foreground))
--apollia-accent    → hsl(var(--primary))
--apollia-danger    → hsl(var(--destructive))
--apollia-success   → hsl(var(--success))
--apollia-warning   → hsl(var(--warning))
--apollia-info      → hsl(var(--info))
```

> **REGLE** : Preferer les classes Tailwind semantiques (`bg-success`, `text-destructive`) aux `var(--apollia-*)` inline. Les aliases existent pour les cas ou inline style est incontournable (ex: SVG).

### 2.3 Tailwind colors

Toutes les couleurs sont definies dans `tailwind.config.ts` via les CSS variables :

```typescript
colors: {
  background, foreground,
  primary: { DEFAULT, foreground },
  secondary: { DEFAULT, foreground },
  destructive: { DEFAULT, foreground },
  muted: { DEFAULT, foreground },
  accent: { DEFAULT, foreground },
  card: { DEFAULT, foreground },
  info: { DEFAULT, foreground },
  warning: { DEFAULT, foreground },
  success: { DEFAULT, foreground },
}
```

---

## 3. Glass Morphism — 5 niveaux

Chaque niveau correspond a une profondeur visuelle. Utiliser le bon niveau selon le contexte :

| Classe | Blur | Opacite | Cas d'usage |
|---|---|---|---|
| `.glass-panel` | `backdrop-blur-2xl` | 90% | Sidebar, sheets, overlays |
| `.glass-card` | `backdrop-blur-xl` | 90% | Cartes de contenu, detail views |
| `.glass-card-hover` | `backdrop-blur-xl` + hover lift | 90% | Cartes interactives (agent, trigger, pipeline) |
| `.glass-surface` | `backdrop-blur-md` | 50% | Tags, headers de table, conteneurs legers |
| `.glass-inset` | `backdrop-blur-sm` | — | Hover states, elements imbriques |

### Bordures glass

| Classe | Opacite | Usage |
|---|---|---|
| `.glass-border` | 8% (light) / 10% (dark) | Bordures visibles sur les cartes |
| `.glass-border-subtle` | 5% (light) / 6% (dark) | Bordures subtiles, separateurs |

### Hover sur glass-card-hover

```css
/* Effet hover automatique (defini dans app.css) */
.glass-card-hover:hover {
  transform: translateY(-1px);
  /* Ombres intensifiees avec brand glow subtil */
}
```

> **REGLE** : Ne JAMAIS ecrire de box-shadow inline. Utiliser les classes glass-* qui gerent light/dark automatiquement.

---

## 4. Typographie

### 4.1 Police

**Inter** via `@fontsource/inter`, definie dans `tailwind.config.ts` :
```typescript
fontFamily: { sans: ["Inter", ...defaultTheme.fontFamily.sans] }
```

### 4.2 Echelle typographique

| Element | Classe Tailwind | Poids | Usage |
|---|---|---|---|
| Titre de page (h1) | `text-2xl font-semibold tracking-tight` | 600 | Un seul par page |
| Titre de section (h2) | `text-sm font-medium uppercase tracking-wider text-muted-foreground` | 500 | Headers de groupe |
| Titre de carte | `text-[13px] font-medium` | 500 | Nom d'agent, titre de trigger |
| Titre detail | `text-base font-medium` | 500 | Headers dans sheets/dialogs |
| Corps | `text-xs` ou `text-[13px]` | 400 | Texte principal des cartes |
| Label | `text-[11px] text-muted-foreground` | 400 | Meta-informations, timestamps |
| Label formulaire | `text-xs font-medium text-muted-foreground` | 500 | Au-dessus des inputs |
| Helper text | `text-[10px] text-muted-foreground/50` | 400 | Sous les inputs |
| Code/ID | `text-[9px] font-mono text-muted-foreground/40` | 400 | Identifiants techniques |

### 4.3 Regles de poids

| Poids | Quand l'utiliser | Quand NE PAS l'utiliser |
|---|---|---|
| **400 (normal)** | Corps, labels, descriptions | — |
| **500 (medium)** | Titres de carte, labels de formulaire, badges, section headers | — |
| **600 (semibold)** | Titres de page (h1) UNIQUEMENT | Tout le reste |
| **700 (bold)** | JAMAIS en production | — |

> **REGLE** : `font-bold` est interdit. Utiliser `font-semibold` pour les h1 uniquement, `font-medium` pour tout le reste.

---

## 5. Composants primitifs

Tous les composants sont dans `lib/components/ui/`. Utiliser ces primitives systematiquement — ne JAMAIS utiliser d'elements HTML natifs non styles.

### 5.1 Button

**Fichier** : `ui/button/Button.svelte`
**Import** : `import { Button } from "$lib/components/ui/button";`

| Prop | Type | Default | Description |
|---|---|---|---|
| `variant` | `"default" \| "destructive" \| "outline" \| "secondary" \| "ghost" \| "link"` | `"default"` | Style visuel |
| `size` | `"default" \| "sm" \| "lg" \| "icon"` | `"default"` | Taille |

**Classes de base** :
```
inline-flex items-center justify-center whitespace-nowrap rounded-md text-sm font-medium
ring-offset-background transition-colors duration-150
focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2
disabled:pointer-events-none disabled:opacity-50 active:scale-[0.98]
```

**Variants** :
- `default` : `bg-primary text-primary-foreground shadow-sm hover:bg-primary/90`
- `destructive` : `bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90`
- `outline` : `border border-border bg-transparent text-foreground hover:bg-muted`
- `secondary` : `bg-muted text-foreground hover:bg-muted/80`
- `ghost` : `text-foreground hover:bg-muted`
- `link` : `text-primary underline-offset-4 hover:underline`

**Tailles** :
- `default` : `h-10 px-4 py-2`
- `sm` : `h-9 rounded-md px-3`
- `lg` : `h-11 rounded-md px-8`
- `icon` : `h-10 w-10`

### 5.2 Card

**Fichier** : `ui/card/Card.svelte` + `CardHeader.svelte` + `CardContent.svelte` + `CardTitle.svelte` + `CardDescription.svelte`
**Import** : `import { Card, CardHeader, CardContent, CardTitle, CardDescription } from "$lib/components/ui/card";`

| Prop | Type | Default | Description |
|---|---|---|---|
| `interactive` | `boolean` | `false` | `true` = glass-card-hover, `false` = glass-card |

**Pattern carte standard** :
```svelte
<Card interactive data-testid="entity-card">
  <!-- Barre d'accent statut -->
  <div class="h-0.5 w-full bg-{status-color}"></div>
  <div class="px-3.5 pt-3 pb-2.5 flex-1 flex flex-col">
    <!-- Contenu -->
  </div>
</Card>
```

### 5.3 Badge

**Fichier** : `ui/badge/Badge.svelte`
**Import** : `import { Badge } from "$lib/components/ui/badge";`

| Variant | Classes |
|---|---|
| `default` | `bg-primary/10 text-primary dark:bg-primary/20` |
| `secondary` | `bg-muted text-muted-foreground` |
| `destructive` | `bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-300` |
| `outline` | `border border-border text-foreground` |
| `success` | `bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300` |
| `warning` | `bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-300` |
| `info` | `bg-sky-50 text-sky-700 dark:bg-sky-950/30 dark:text-sky-300` |

Base : `inline-flex items-center rounded-full border border-transparent px-2.5 py-0.5 text-xs font-medium`

### 5.4 Input

**Fichier** : `ui/input/Input.svelte`
**Import** : `import { Input } from "$lib/components/ui/input";`

Classes :
```
flex h-10 w-full rounded-md border border-border bg-background px-3 py-2 text-sm
ring-offset-background transition-shadow duration-150
placeholder:text-muted-foreground
focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50
disabled:cursor-not-allowed disabled:opacity-50
```

### 5.5 Textarea

**Fichier** : `ui/textarea/Textarea.svelte`
**Import** : `import { Textarea } from "$lib/components/ui/textarea";`

| Prop | Type | Description |
|---|---|---|
| `value` | `string` (bindable) | Contenu |

Meme style que Input + `min-h-[80px] resize-y`.

### 5.6 Select

**Fichier** : `ui/select/Select.svelte`
**Import** : `import { Select } from "$lib/components/ui/select";`

| Prop | Type | Description |
|---|---|---|
| `value` | `string` (bindable) | Valeur selectionnee |

Wrapper `<div class="relative">` avec `<select>` style + chevron `ChevronDown` lucide.
Meme focus ring que Input.

```svelte
<Select bind:value={selectedValue}>
  <option value="">Choisir...</option>
  <option value="a">Option A</option>
</Select>
```

### 5.7 Checkbox

**Fichier** : `ui/checkbox/Checkbox.svelte`
**Import** : `import { Checkbox } from "$lib/components/ui/checkbox";`

| Prop | Type | Description |
|---|---|---|
| `checked` | `boolean` (bindable) | Etat |
| `onchange` | `(checked: boolean) => void` | Callback |
| `disabled` | `boolean` | Desactive |

Rendu : bouton `role="checkbox"` avec `aria-checked`. Icone `Check` de lucide quand coche.
Taille : `h-4 w-4`, border-radius `rounded-[3px]`.

### 5.8 Toggle (Switch)

**Fichier** : `ui/toggle/Toggle.svelte`
**Import** : `import { Toggle } from "$lib/components/ui/toggle";`

| Prop | Type | Description |
|---|---|---|
| `checked` | `boolean` (bindable) | Etat on/off |
| `size` | `"sm" \| "default"` | Taille |

Rendu : bouton `role="switch"` avec `aria-checked`. Dot qui slide avec `transition-transform duration-150`.

| Taille | Track | Dot | Translate |
|---|---|---|---|
| `sm` | `h-4 w-7` | `h-3 w-3` | `translate-x-3` |
| `default` | `h-5 w-9` | `h-4 w-4` | `translate-x-4` |

### 5.9 RadioGroup + RadioItem

**Fichiers** : `ui/radio/RadioGroup.svelte` + `RadioItem.svelte`
**Import** : `import { RadioGroup, RadioItem } from "$lib/components/ui/radio";`

RadioGroup : `flex flex-col gap-2` avec `role="radiogroup"`.
RadioItem : cercle `h-4 w-4 rounded-full border`, dot interieur `h-1.5 w-1.5 rounded-full bg-primary-foreground`.

### 5.10 Dialog

**Fichier** : `ui/dialog/Dialog.svelte`
**Import** : `import { Dialog } from "$lib/components/ui/dialog";`

| Prop | Type | Default | Description |
|---|---|---|---|
| `open` | `boolean` | — | Affichage |
| `onclose` | `() => void` | — | Fermeture |
| `size` | `"sm" \| "md" \| "lg"` | `"md"` | Largeur |
| `title` | `string?` | — | Titre avec bordure basse |

**Tailles** : sm = `440px`, md = `520px`, lg = `620px`
**Animations** : backdrop `fade(200ms)`, dialog `scale(start: 0.97, 200ms)`
**Fermeture** : Escape + clic backdrop + bouton X
**Accessibilite** : `role="dialog"` + `aria-modal="true"` + `aria-label`

### 5.11 Sheet

**Fichier** : `ui/sheet/Sheet.svelte`
**Import** : `import { Sheet } from "$lib/components/ui/sheet";`

| Prop | Type | Description |
|---|---|---|
| `open` | `boolean` | Affichage |
| `onclose` | `() => void` | Fermeture |

Panneau lateral droit `w-[400px]`, `glass-panel border-l`.
**Animation** : `fly({ x: 400, duration: 250 })` avec easing cubic-out.
**Fermeture** : Escape + clic backdrop + bouton X.

### 5.12 Toast + ToastContainer

**Fichiers** : `ui/toast/Toast.svelte` + `ToastContainer.svelte` + `store.ts`
**Store** : `import { addToast } from "$lib/components/ui/toast/store";`

**API du store** :
```typescript
addToast(message: string, variant?: "success" | "error" | "info")
removeToast(id: string)
```

**Variants** :
- `success` : icone `CheckCircle` vert
- `error` : icone `AlertCircle` rouge
- `info` : icone `Info` bleu

**Auto-dismiss** : 4 secondes.
**Animation** : `fly({ y: -8, duration: 200 })`.

> **REGLE** : Ne JAMAIS implementer de toast inline dans un composant. Toujours utiliser `addToast()` depuis le store.

### 5.13 Separator

**Fichier** : `ui/separator/Separator.svelte`

Horizontal : `shrink-0 bg-border h-[1px] w-full`
Vertical : `shrink-0 bg-border h-full w-[1px]`

### 5.14 Skeleton

**Fichier** : `ui/skeleton/Skeleton.svelte`

Classes : `animate-pulse rounded-md bg-muted`
Props : `width`, `height` via style inline.

---

## 6. Structure de page

### 6.1 Container

Chaque page utilise :

```svelte
<div class="max-w-6xl" data-testid="{page-name}-page">
  <!-- Contenu -->
</div>
```

### 6.2 Header

```svelte
<div class="flex items-center justify-between">
  <div>
    <h1 class="text-2xl font-semibold tracking-tight">{$t('page.title')}</h1>
    <p class="text-xs text-muted-foreground">{$t('page.subtitle')}</p>
  </div>
  <Button size="sm">
    <Icon size={14} class="mr-1.5" />
    {$t('page.cta')}
  </Button>
</div>
```

### 6.3 Sections

```svelte
<div class="space-y-6">
  <!-- Section avec titre -->
  <div>
    <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground mb-3">
      {$t('section.title')}
    </h2>
    <!-- Contenu -->
  </div>
</div>
```

### 6.4 Grilles de cartes

```svelte
<div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
  {#each items as item (item.id)}
    <ItemCard {item} />
  {/each}
</div>
```

### 6.5 Etats vides

Utiliser le composant `EmptyState` :

```svelte
<EmptyState
  icon={PackageOpen}
  title={$t('empty.title')}
  subtitle={$t('empty.subtitle')}
>
  <Button size="sm">{$t('empty.cta')}</Button>
</EmptyState>
```

Alternative legere pour les zones secondaires :
```svelte
<div class="glass-surface border-dashed rounded-lg p-8 text-center">
  <Icon size={40} class="mx-auto mb-3 text-muted-foreground/40" />
  <p class="text-sm text-muted-foreground">{$t('empty.message')}</p>
</div>
```

### 6.6 Convention data-testid

Format : `{page}-{action}-{entity}`

Exemples :
- `triggers-page` — conteneur page
- `triggers-create-btn` — bouton CTA
- `trigger-card-{id}` — carte individuelle
- `trigger-delete-confirm` — bouton confirmation suppression

---

## 7. Pattern Card

### 7.1 Carte interactive standard

```svelte
<Card interactive data-testid="entity-card-{id}">
  <!-- Barre d'accent statut (h-0.5) -->
  <div class="h-0.5 w-full {statusColor}"></div>

  <!-- Contenu principal -->
  <div class="px-3.5 pt-3 pb-2.5 flex-1 flex flex-col">
    <!-- Ligne 1 : Avatar + Titre + Badge -->
    <div class="flex items-center gap-2.5">
      <div
        style="background: hsl({hue}, 60%, 48%); box-shadow: 0 2px 8px -1px hsla({hue}, 60%, 38%, 0.3);"
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-xs font-semibold text-white"
      >
        {initial}
      </div>
      <div class="min-w-0 flex-1">
        <p class="text-[13px] font-medium truncate">{name}</p>
        <p class="text-[11px] text-muted-foreground">{subtitle}</p>
      </div>
      <Badge variant={statusVariant}>{statusLabel}</Badge>
    </div>

    <!-- Contenu additionnel -->
    <p class="mt-2 text-xs text-muted-foreground line-clamp-2">{description}</p>
  </div>

  <!-- Footer optionnel (actions) -->
  <div class="border-t border-border/50 px-3.5 py-2 flex items-center justify-end gap-1">
    <Button variant="ghost" size="icon" class="h-6 w-6">
      <Icon size={13} />
    </Button>
  </div>
</Card>
```

### 7.2 Couleurs de barre d'accent

| Statut | Classe |
|---|---|
| Actif / Pret | `bg-primary` |
| En cours | `bg-primary` |
| Complete / Succes | `bg-success` |
| Degrade / Avertissement | `bg-warning` |
| Erreur / Echec | `bg-destructive` |
| Inactif / Arrete | `bg-muted-foreground/20` |
| En attente approbation | `bg-warning` |

### 7.3 Avatar deterministe

```typescript
function avatarHue(name: string): number {
  return name.split("").reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360;
}
```

Style : `hsl({hue}, 60%, 48%)` fond, `hsla({hue}, 60%, 38%, 0.3)` ombre.

---

## 8. Animations

### 8.1 Courbe de reference

**Apple ease** : `cubic-bezier(0.2, 0, 0, 1)` — disponible via `ease-apple` dans Tailwind.

### 8.2 Entrees de page

```svelte
<!-- Transition de route (dans Main.svelte) -->
{#key $currentRoute}
  <div transition:fade={{ duration: 150 }}>
    <!-- Composant de page -->
  </div>
{/key}
```

### 8.3 Entrees de liste

```svelte
{#each items as item, i (item.id)}
  <div
    in:fly={{ y: 4, duration: 200, delay: i * 30 }}
    animate:flip={{ duration: 250 }}
  >
    <ItemCard {item} />
  </div>
{/each}
```

### 8.4 Classes d'animation CSS

| Classe | Effet | Duree |
|---|---|---|
| `animate-fade-in` | `opacity 0→1` | 200ms |
| `animate-slide-up` | `opacity 0→1` + `translateY(8px→0)` | 300ms |
| `animate-scale-in` | `opacity 0→1` + `scale(0.96→1)` | 250ms |
| `animate-slide-in-right` | `translateX(100%→0)` | 300ms |
| `animate-glow-pulse` | Glow brand subtil | 2s infinite |

### 8.5 Transitions interactives

| Element | Propriete | Duree |
|---|---|---|
| Button hover | `transition-colors` | `duration-150` |
| Button active | `scale(0.98)` | Immediat |
| Card hover | `box-shadow, transform` | 250ms ease-apple |
| Focus ring | `transition-shadow` | `duration-150` |
| Toggle dot | `transition-transform` | `duration-150` |
| Theme switch | `background-color, color` sur body | 300ms/200ms |

### 8.6 Transitions de composants

| Composant | Entree | Sortie |
|---|---|---|
| Dialog | `scale(0.97→1, 200ms)` | `scale(1→0.97, 200ms)` |
| Sheet | `fly(x: 400, 250ms, cubic-out)` | Inverse |
| Toast | `fly(y: -8, 200ms)` | Inverse |
| Backdrop | `fade(200ms)` | `fade(200ms)` |

---

## 9. Couleurs semantiques

### 9.1 Regles d'utilisation

| Besoin | Classe Tailwind | NE PAS utiliser |
|---|---|---|
| Texte succes | `text-success` | `text-[var(--apollia-success)]`, `text-green-500` |
| Fond succes | `bg-success/10` | `bg-[var(--apollia-success)]/10` |
| Bordure succes | `border-success` | `border-[var(--apollia-success)]` |
| Badge succes | `<Badge variant="success">` | Classes inline |
| Texte destructif | `text-destructive` | `text-red-500` |
| Hover fond | `hover:bg-muted` | `hover:bg-[rgba(52,53,245,0.04)]` |

### 9.2 Couleurs de statut (mapping)

| Statut | Badge variant | Accent bar | Icone |
|---|---|---|---|
| Active / Ready | `success` | `bg-primary` | `CheckCircle` |
| Working / Running | `info` | `bg-primary` | `Loader2 animate-spin` |
| Completed | `success` | `bg-success` | `CheckCircle` |
| Failed | `destructive` | `bg-destructive` | `XCircle` |
| Degraded | `warning` | `bg-warning` | `AlertTriangle` |
| Stopped / Inactive | `secondary` | `bg-muted-foreground/20` | `Circle` |
| Pending approval | `warning` | `bg-warning` | `Clock` |

---

## 10. Accessibilite

### 10.1 Focus visible

Tous les elements interactifs doivent avoir :
```
focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2
```

### 10.2 Roles ARIA

| Composant | Role | Attribut |
|---|---|---|
| Checkbox | `role="checkbox"` | `aria-checked` |
| Toggle | `role="switch"` | `aria-checked` |
| Dialog | `role="dialog"` | `aria-modal="true"` + `aria-label` |
| Sheet | `role="dialog"` | `aria-modal="true"` |
| RadioGroup | `role="radiogroup"` | — |

### 10.3 Navigation clavier

- **Escape** ferme tout Dialog/Sheet ouvert
- **Space/Enter** toggle les Checkbox/Toggle/RadioItem
- **Tab** cycle entre les elements interactifs
- Dialog/Sheet : le focus est emprisonne (focus trap)

### 10.4 data-testid

Obligatoire sur :
- Chaque conteneur de page (`{page}-page`)
- Chaque bouton CTA (`{page}-{action}-btn`)
- Chaque carte individuelle (`{entity}-card-{id}`)
- Chaque bouton de confirmation (`{entity}-{action}-confirm`)
- Chaque dialog close (`dialog-close`, `sheet-close`)

---

## 11. Internationalisation (i18n)

### 11.1 Pattern

```svelte
<script>
  import { t } from "svelte-i18n";
</script>

<h1>{$t('agents.title')}</h1>
<p>{$t('agents.description', { values: { count: 5 } })}</p>
```

### 11.2 Fichiers

- `src/lib/i18n/en.json` — Anglais (reference)
- `src/lib/i18n/fr.json` — Francais

### 11.3 Convention de cles

```
{page}.{section}.{element}
```

Exemples :
```json
{
  "triggers.title": "Triggers",
  "triggers.subtitle": "Manage your automation triggers",
  "triggers.create": "Create Trigger",
  "triggers.empty.title": "No triggers configured",
  "triggers.empty.subtitle": "Create your first trigger to automate agent execution",
  "common.actions.delete": "Delete",
  "common.actions.cancel": "Cancel",
  "common.confirm.delete.title": "Confirm deletion",
  "common.confirm.delete.message": "This action cannot be undone."
}
```

### 11.4 Regles

- Toute string visible par l'utilisateur doit etre dans les fichiers i18n
- Les deux langues (EN + FR) doivent etre presentes pour chaque cle
- Les cles communes vont dans `common.*`
- Les cles specifiques a une page vont dans `{page}.*`

---

## 12. Tables

### 12.1 Pattern table standard

```svelte
<div class="glass-card glass-border rounded-lg overflow-hidden">
  <table class="w-full">
    <thead>
      <tr class="border-b border-border bg-muted/50">
        <th class="px-3 py-2 text-left text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          {$t('column.name')}
        </th>
      </tr>
    </thead>
    <tbody class="divide-y divide-border/40">
      {#each rows as row}
        <tr class="hover:bg-muted transition-colors">
          <td class="px-3 py-2.5 text-xs">{row.value}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
```

### 12.2 Regles

- Wrapper : `glass-card glass-border rounded-lg overflow-hidden`
- Headers : `bg-muted/50`, `text-[10px] font-medium uppercase tracking-wider`
- Rows : `divide-y divide-border/40`, `hover:bg-muted`
- Ne JAMAIS utiliser `hover:bg-[rgba(...)]` hardcode

---

## 13. Formulaires dans les Dialogs

### 13.1 Pattern champ

```svelte
<div class="space-y-1.5">
  <label for="field-id" class="block text-xs font-medium text-muted-foreground">
    {$t('form.field.label')}
  </label>
  <Input id="field-id" bind:value={fieldValue} placeholder={$t('form.field.placeholder')} />
  {#if error}
    <p class="text-xs text-destructive">{error}</p>
  {/if}
</div>
```

### 13.2 Footer d'actions

```svelte
<div class="flex justify-end gap-2 pt-4">
  <Button variant="outline" onclick={handleCancel}>{$t('common.actions.cancel')}</Button>
  <Button onclick={handleSubmit} disabled={submitting}>
    {submitting ? $t('common.actions.saving') : $t('common.actions.save')}
  </Button>
</div>
```

### 13.3 Regles

- **Inputs** : Toujours utiliser `<Input>`, jamais `<input>`
- **Selects** : Toujours utiliser `<Select>`, jamais `<select>`
- **Checkboxes** : Toujours utiliser `<Checkbox>`, jamais `<input type="checkbox">`
- **Radios** : Toujours utiliser `<RadioGroup>` + `<RadioItem>`, jamais `<input type="radio">`
- **Textareas** : Toujours utiliser `<Textarea>`, jamais `<textarea>`
- **Toggles** : Toujours utiliser `<Toggle>`, jamais de toggle hand-built

---

## 14. Confirmations destructives

Utiliser le composant `ConfirmDialog` (STORY-212, Sprint 19) pour toute action destructive :

```svelte
<ConfirmDialog
  open={showDeleteConfirm}
  onclose={() => showDeleteConfirm = false}
  onconfirm={handleDelete}
  title={$t('common.confirm.delete.title')}
  message={$t('common.confirm.delete.message')}
  confirmLabel={$t('common.actions.delete')}
  loading={deleting}
/>
```

> **REGLE** : Ne JAMAIS implementer de modal de confirmation inline (`fixed inset-0 z-50`). Toujours utiliser `ConfirmDialog` ou `Dialog`.

---

## Fichiers de reference

| Fichier | Role |
|---|---|
| `crates/apollia-desktop/ui/src/app.css` | Tokens CSS, glass classes, animations |
| `crates/apollia-desktop/ui/src/lib/design-tokens.ts` | Tokens TypeScript |
| `crates/apollia-desktop/ui/tailwind.config.ts` | Extensions Tailwind |
| `crates/apollia-desktop/ui/src/lib/components/ui/` | Composants primitifs |
| `crates/apollia-desktop/ui/src/components/agents/AgentCard.svelte` | Exemple de reference : carte interactive |
| `crates/apollia-desktop/ui/src/routes/Dashboard.svelte` | Exemple de reference : structure de page |
