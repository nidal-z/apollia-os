# Apollia OS — Comprehensive Design System Documentation

**Version:** 2.0
**Last Updated:** 2026-04-27 (based on live codebase exploration)
**Stack:** Tauri v2 + Svelte 5 + Tailwind 3.4 + lucide-svelte + bits-ui
**Design Tokens:** ADR-077 (Design tokens v2 — elevation, warmth dark, rim lights)
**Reference:** `crates/apollia-desktop/ui/src/`

---

## 1. COLOR PALETTE & CSS VARIABLES

### 1.1 Light Mode (`:root`)

All colors defined as HSL custom properties in `app.css`. Hex values are approximations.

| Variable | HSL Value | Hex (approx) | Usage |
|----------|-----------|--------------|-------|
| `--primary` | `240 91% 58%` | `#3435f5` | Main accent, CTAs, links, focus rings |
| `--primary-foreground` | `0 0% 100%` | `#ffffff` | Text on primary backgrounds |
| `--primary-hover` | `240 91% 54%` | `#2d2ce6` | Primary button hover state |
| `--primary-active` | `240 91% 50%` | `#2626d8` | Primary button active/pressed state |
| `--secondary` | `260 60% 61%` | `#7c5fd6` | Secondary accent (rare) |
| `--secondary-foreground` | `0 0% 100%` | `#ffffff` | Text on secondary |
| `--accent` | `240 91% 58%` | `#3435f5` | Alias for primary |
| `--accent-foreground` | `0 0% 100%` | `#ffffff` | Text on accent |
| `--background` | `38 28% 90%` | `#e8e1d1` | Page/app background (warm cream) |
| `--foreground` | `230 15% 14%` | `#1f2029` | Primary text color |
| `--surface-1` | `40 35% 96%` | `#faf6ec` | Elevated card surfaces, modals |
| `--surface-2` | `38 30% 92%` | `#f1ecdf` | Default card/panel background |
| `--surface-3` | `36 22% 86%` | `#e8e4d8` | Recessed/inset regions |
| `--card` | `40 35% 96%` | `#faf6ec` | Card background (alias for surface-1) |
| `--card-foreground` | `230 15% 14%` | `#1f2029` | Text on cards |
| `--popover` | `40 35% 96%` | `#faf6ec` | Popover backgrounds |
| `--popover-foreground` | `230 15% 14%` | `#1f2029` | Text in popovers |
| `--muted` | `36 20% 85%` | `#ddd7cb` | Desaturated background (tags, headers) |
| `--muted-foreground` | `220 10% 40%` | `#5c6370` | Secondary/tertiary text |
| `--border` | `36 16% 80%` | `#d1cbc0` | Border color |
| `--input` | `36 14% 84%` | `#dad5cb` | Input field background |
| `--ring` | `240 91% 58%` | `#3435f5` | Focus ring color |
| `--destructive` | `0 72% 40%` | `#c93232` | Error/delete state (darker for WCAG) |
| `--destructive-foreground` | `0 0% 98%` | `#faf7f5` | Text on destructive |
| `--success` | `152 56% 28%` | `#2fa87a` | Success state |
| `--success-foreground` | `152 56% 20%` | `#1f8659` | Text on success |
| `--warning` | `38 92% 28%` | `#d68a00` | Warning/caution state (darker for WCAG) |
| `--warning-foreground` | `32 80% 30%` | `#cc7a1a` | Text on warning |
| `--info` | `213 94% 47%` | `#0572ea` | Information state |
| `--info-foreground` | `213 80% 30%` | `#004a99` | Text on info |

#### Text Tokens (A11y-verified ≥4.5:1 contrast)

| Variable | HSL Value | Usage |
|----------|-----------|-------|
| `--text-muted` | `220 10% 34%` | Secondary prose |
| `--text-success` | `152 60% 22%` | Success messages (darker than `--success`) |
| `--text-warning` | `30 90% 22%` | Warning prose (darker than `--warning`) |
| `--text-danger` | `0 72% 36%` | Error prose (darker than `--destructive`) |

#### Gradients (Light Mode)

| Variable | Value | Usage |
|----------|-------|-------|
| `--primary-gradient-from` | `240 91% 58%` | Gradient start (primary blue) |
| `--primary-gradient-to` | `260 60% 61%` | Gradient end (secondary violet) |
| `--gradient-primary` | `linear-gradient(135deg, hsl(var(--primary-gradient-from)) 0%, hsl(var(--primary-gradient-to)) 100%)` | Hero CTAs, promoted cards |
| `--gradient-surface` | `linear-gradient(180deg, hsl(var(--surface-1)) 0%, hsl(var(--surface-2)) 100%)` | Panel/section backgrounds |
| `--gradient-accent` | `linear-gradient(135deg, hsl(var(--primary) / 0.10) 0%, hsl(var(--secondary) / 0.05) 100%)` | Empty states, accent regions |

---

### 1.2 Dark Mode (`.dark`)

Same tokens, different values. All resolved via `hsl(var(--name))` automatically.

| Variable | HSL Value | Usage Note |
|----------|-----------|-----------|
| `--background` | `28 8% 9%` | **Warm charcoal**, not neutral grey (F.33) |
| `--foreground` | `32 15% 92%` | Off-white with warmth |
| `--surface-1` | `28 10% 14%` | Elevated surfaces dark |
| `--surface-2` | `28 11% 13%` | Card resting state |
| `--surface-3` | `28 8% 10%` | Recessed regions |
| `--card` | `28 10% 14%` | Dark card backgrounds |
| `--card-foreground` | `32 15% 92%` | Text on dark cards |
| `--muted` | `28 7% 18%` | Desaturated dark |
| `--muted-foreground` | `30 8% 62%` | Secondary text dark |
| `--border` | `28 8% 22%` | Dark mode borders |
| `--input` | `28 7% 18%` | Dark input backgrounds |
| `--ring` | `240 91% 62%` | Brighter primary for dark contrast |
| `--primary` | `240 91% 62%` | Boosted for dark contrast |
| `--primary-hover` | `240 91% 66%` | Lighter hover |
| `--primary-active` | `240 91% 58%` | Same as light (consistency) |
| `--secondary` | `260 60% 65%` | Boosted secondary |
| `--destructive` | `0 65% 58%` | Lighter destructive for contrast |
| `--destructive-foreground` | `0 0% 98%` | Off-white text |
| `--success` | `152 50% 50%` | Boosted success |
| `--success-foreground` | `152 50% 82%` | Lighter text on success |
| `--warning` | `38 92% 55%` | Boosted warning |
| `--warning-foreground` | `45 90% 82%` | Lighter text on warning |
| `--info` | `213 94% 60%` | Boosted info |
| `--info-foreground` | `213 90% 82%` | Lighter text on info |

#### Dark Text Tokens

| Variable | HSL Value |
|----------|-----------|
| `--text-muted` | `30 8% 72%` |
| `--text-success` | `152 55% 68%` |
| `--text-warning` | `40 95% 72%` |
| `--text-danger` | `0 75% 72%` |

---

### 1.3 Glass Border Tokens

Explicit light/dark variants to handle glassmorphism borders correctly in both themes.

| Variable | Light Value | Dark Value | Usage |
|----------|-------------|-----------|-------|
| `--glass-border-light` | `220 80% 58% / 0.10` | — | Light mode border at rest |
| `--glass-border-light-hover` | `220 80% 58% / 0.18` | — | Light mode border on hover |
| `--glass-border-dark` | `260 40% 70% / 0.12` | — | Dark mode border at rest |
| `--glass-border-dark-hover` | `260 40% 70% / 0.22` | — | Dark mode border on hover |
| `--glass-border` | `var(--glass-border-light)` | `var(--glass-border-dark)` | Canonical glass border (auto-resolved) |
| `--glass-border-hover` | `var(--glass-border-light-hover)` | `var(--glass-border-dark-hover)` | Canonical on hover |
| `--glass-inset` | `220 80% 58% / 0.06` | `32 40% 78% / 0.14` | Inset rim light (warm-tinted dark) |

---

### 1.4 Chart Palette (8 colors for data visualization)

**Light mode:**
```
--chart-1: #7c3aed (purple)
--chart-2: #3b82f6 (blue)
--chart-3: #10b981 (green)
--chart-4: #f59e0b (amber)
--chart-5: #ef4444 (red)
--chart-6: #ec4899 (pink)
--chart-7: #06b6d4 (cyan)
--chart-8: #84cc16 (lime)
```

**Dark mode:** (lighter variants)
```
--chart-1: #a78bfa
--chart-2: #60a5fa
--chart-3: #34d399
--chart-4: #fbbf24
--chart-5: #f87171
--chart-6: #f472b6
--chart-7: #22d3ee
--chart-8: #a3e635
```

---

## 2. ELEVATION & SHADOW SYSTEM

### 2.1 Elevation Scale (5 levels with rim light)

Each level uses **multi-layer shadows + inset rim light** for materiality. Light mode uses warm-tinted rim; dark mode uses bronze-white.

**Light mode — shadow tokens:**

```css
--shadow-elev-0:
  0 1px 0 rgba(120, 100, 60, 0.04),
  inset 0 1px 0 rgba(255, 252, 240, 0.5);

--shadow-elev-1:
  0 1px 2px rgba(120, 100, 60, 0.06),
  0 2px 6px -1px rgba(120, 100, 60, 0.05),
  inset 0 1px 0 rgba(255, 252, 240, 0.6);

--shadow-elev-2:
  0 1px 2px rgba(120, 100, 60, 0.06),
  0 4px 12px rgba(120, 100, 60, 0.08),
  inset 0 1px 0 rgba(255, 252, 240, 0.6);

--shadow-elev-3:
  0 2px 4px rgba(120, 100, 60, 0.07),
  0 10px 28px -6px rgba(120, 100, 60, 0.12),
  inset 0 1px 0 rgba(255, 252, 240, 0.7);

--shadow-elev-4:
  0 4px 8px rgba(120, 100, 60, 0.09),
  0 24px 56px -12px rgba(52, 53, 245, 0.18),
  inset 0 1px 0 rgba(255, 252, 240, 0.8);
```

**Dark mode — shadow tokens:**

```css
--shadow-elev-0:
  0 1px 0 rgba(0, 0, 0, 0.20),
  inset 0 1px 0 hsl(32 30% 70% / 0.10);

--shadow-elev-1:
  0 1px 2px rgba(0, 0, 0, 0.35),
  0 2px 6px -1px rgba(0, 0, 0, 0.25),
  inset 0 1px 0 hsl(32 30% 70% / 0.12);

--shadow-elev-2:
  0 2px 4px rgba(0, 0, 0, 0.35),
  0 6px 16px -2px rgba(0, 0, 0, 0.35),
  inset 0 1px 0 hsl(32 30% 70% / 0.14);

--shadow-elev-3:
  0 4px 8px rgba(0, 0, 0, 0.40),
  0 14px 36px -8px rgba(0, 0, 0, 0.50),
  inset 0 1px 0 hsl(32 30% 70% / 0.16);

--shadow-elev-4:
  0 8px 16px rgba(0, 0, 0, 0.45),
  0 28px 72px -16px rgba(52, 53, 245, 0.35),
  inset 0 1px 0 hsl(32 30% 70% / 0.18);
```

| Level | Use Case | Tailwind Class | CSS Variable |
|-------|----------|----------------|--------------|
| 0 | Flat with rim light (table rows, inline surfaces) | `shadow-elev-0` | `var(--shadow-elev-0)` |
| 1 | Cards, buttons at rest | `shadow-elev-1` | `var(--shadow-elev-1)` |
| 2 | Raised cards, menus, popovers | `shadow-elev-2` | `var(--shadow-elev-2)` |
| 3 | Overlays, sheets, hover states | `shadow-elev-3` | `var(--shadow-elev-3)` |
| 4 | Hero modals, spotlight | `shadow-elev-4` | `var(--shadow-elev-4)` |

### 2.2 Primary-tinted Shadows

For CTA buttons and promoted components. Each level includes primary blue tint.

```css
--shadow-primary-sm: 0 2px 8px -1px hsl(240 91% 58% / 0.30);
--shadow-primary-md: 0 4px 16px -2px hsl(240 91% 58% / 0.35);
--shadow-primary-lg: 0 6px 24px -2px hsl(240 91% 58% / 0.40);
--shadow-primary-xl: 0 12px 48px -8px hsl(260 60% 61% / 0.35);
```

Dark mode boosts opacity:
```css
--shadow-primary-sm: 0 2px 8px -1px hsl(240 91% 62% / 0.45);
--shadow-primary-md: 0 4px 16px -2px hsl(240 91% 62% / 0.50);
--shadow-primary-lg: 0 6px 24px -2px hsl(240 91% 62% / 0.55);
--shadow-primary-xl: 0 12px 48px -8px hsl(260 60% 65% / 0.50);
```

| Use | Tailwind | CSS Variable |
|-----|----------|--------------|
| Button at rest | `shadow-primary-sm` | `var(--shadow-primary-sm)` |
| Button hover | `shadow-primary-md` | `var(--shadow-primary-md)` |
| Hero button hover | `shadow-primary-lg` | `var(--shadow-primary-lg)` |
| Featured button | `shadow-primary-xl` | `var(--shadow-primary-xl)` |

### 2.3 Semantic Shadows

Success and destructive actions get their own colored shadows:

```css
--shadow-success-md: 0 4px 16px -2px hsl(152 56% 35% / 0.35);
--shadow-destructive-md: 0 4px 16px -2px hsl(0 72% 40% / 0.30);
```

---

## 3. GLASS MORPHISM & SURFACES

### 3.1 Glass Layers

Five canonical layers for depth and context. Each uses `backdrop-blur` + a brand-tinted gradient + elevation shadow.

| Class | Blur | Background | Elevation | Use Case |
|-------|------|------------|-----------|----------|
| `.glass-panel` | `backdrop-blur-2xl` | Brand gradient + 90% opaque warm cream | `elev-0` | Sidebars, sheets, overlays |
| `.glass-card` | `backdrop-blur-xl` | Subtle brand gradient + 90% opaque cream | `elev-2` | Content cards, detail views |
| `.glass-card-hover` | `backdrop-blur-xl` | Same as glass-card | `elev-2 → 3 on hover` | Interactive cards with lift effect |
| `.glass-surface` | `backdrop-blur-md` | 50% opaque cream wash | — | Tags, table headers, lightweight containers |
| `.glass-inset` | `backdrop-blur-sm` | Canonical glass-inset token (resolves per theme) | — | Nested hover states, subtle backgrounds |

**Light mode backgrounds:**

```css
.glass-panel {
  background:
    linear-gradient(135deg, rgba(52, 53, 245, 0.02) 0%, rgba(124, 95, 214, 0.01) 100%),
    rgba(246, 240, 228, 0.90);
}

.glass-card {
  background:
    linear-gradient(145deg, rgba(52, 53, 245, 0.025) 0%, rgba(124, 95, 214, 0.012) 100%),
    rgba(250, 246, 236, 0.90);
}

.glass-surface {
  background: rgba(240, 234, 220, 0.50);
}
```

**Dark mode backgrounds:**

```css
.dark .glass-panel {
  background:
    linear-gradient(135deg, rgba(52, 53, 245, 0.04) 0%, rgba(124, 95, 214, 0.025) 100%),
    hsl(28 10% 11% / 0.88);
}

.dark .glass-card {
  background:
    linear-gradient(145deg, rgba(52, 53, 245, 0.05) 0%, rgba(124, 95, 214, 0.03) 100%),
    hsl(28 10% 14% / 0.78);
}

.dark .glass-surface {
  background: hsl(28 9% 16% / 0.50);
}

.dark .glass-inset {
  background: hsl(var(--glass-inset));
}
```

### 3.2 Hover Lift & Glow

Cards use a spring-based lift on hover:

```css
.glass-card-hover {
  transition:
    box-shadow var(--motion-slow) var(--ease-apple),
    transform var(--motion-base) var(--ease-apple);
}

.glass-card-hover:hover {
  box-shadow: var(--shadow-elev-3);
  transform: translateY(-2px) scale(1.01);
}

.glass-card-hover:active {
  transform: translateY(0) scale(0.998);
  transition-duration: var(--motion-fast);
}
```

Hover utilities:
- `.hover-lift` — lift + glow for cards
- `.hover-glow` — glow only for CTAs

---

## 4. TYPOGRAPHY

### 4.1 Font Family

**Inter** via `@fontsource/inter`, configured in `tailwind.config.ts`:

```typescript
fontFamily: { sans: ["Inter", ...defaultTheme.fontFamily.sans] }
```

### 4.2 Type Scale

Display types use `clamp()` for fluid sizing between `xs` (375px) and `xl` (1280px):

| Component | Size (clamp) | Weight | Line Height | Letter Spacing | Usage |
|-----------|--------------|--------|------------|-----------------|-------|
| Display XL | `clamp(3rem, 2rem + 3.2vw, 4.5rem)` | 700 | 1.05 | -0.03em | Hero titles (rarely used) |
| Display LG | `clamp(2.25rem, 1.5rem + 2.2vw, 3.25rem)` | 700 | 1.1 | -0.025em | Page heroes |
| Display MD | `clamp(1.75rem, 1.25rem + 1.4vw, 2.25rem)` | 600 | 1.15 | -0.02em | Section headers |
| Display SM | `1.5rem` | 600 | 1.2 | -0.015em | Subsection headers |
| `text-2xl` | 1.5rem | 600 (`font-semibold`) | 1.2 | -0.015em | Page h1 titles |
| `text-base` | 1rem | 500 (`font-medium`) | 1.5 | — | Dialog/sheet headers |
| `text-sm` | 0.875rem | 500 (`font-medium`) | 1.5 | — | Section h2 titles |
| `text-[13px]` | 0.8125rem | 500 (`font-medium`) | 1.5 | — | Card titles |
| `text-xs` | 0.75rem | 400 | 1.5 | — | Body text, card content |
| `text-[11px]` | 0.6875rem | 400 | 1.4 | — | Meta-information, timestamps |
| `text-[10px]` | 0.625rem | 400 | 1.4 | — | Helper text, secondary labels |
| `text-[9px]` (mono) | 0.5625rem | 400 | 1.4 | — | Code, IDs, technical identifiers |

### 4.3 Font Weight Rules

| Weight | Tailwind Class | When to Use | NEVER Use For |
|--------|----------------|-------------|---------------|
| 400 | `font-normal` | Body, descriptions, labels | Titles |
| 500 | `font-medium` | Card titles, form labels, badges, section headers | Page titles |
| 600 | `font-semibold` | Page h1 titles ONLY, display text | Everything else |
| 700 | `font-bold` | **NEVER** — removed from production | — |

---

## 5. SPACING SCALE

Tailwind defaults extended with custom motion tokens. Base unit: 0.25rem (4px).

| Tailwind | Pixels | Use Case |
|----------|--------|----------|
| px (0.5 → 2px) | 2px | Dividers |
| 1 | 4px | Gaps in tight layouts |
| 1.5 | 6px | Small gaps |
| 2 | 8px | Standard gap |
| 2.5 | 10px | Component padding |
| 3 | 12px | Card padding |
| 3.5 | 14px | Card content padding |
| 4 | 16px | Section padding, gaps |
| 6 | 24px | Large gaps |
| 8 | 32px | Section padding |

Motion tokens (not spatial, but timing):
- `duration-fast` / `duration-base` / `duration-slow` → 120ms / 200ms / 320ms

---

## 6. BREAKPOINTS & RESPONSIVE DESIGN

**Canonical breakpoints** (from `tailwind.config.ts`):

| Token | Min-Width | Target | Tailwind Prefix |
|-------|-----------|--------|-----------------|
| `xs` | 375px | Operator mobile (iPhone SE minimum) | `xs:` |
| `sm` | 640px | Tablet portrait, narrow desktop | `sm:` |
| `md` | 768px | Tablet landscape, split-screen | `md:` |
| `lg` | 1024px | Laptop standard (builder reference) | `lg:` |
| `xl` | 1280px | Desktop large | `xl:` |
| `2xl` | 1536px | Desktop ultra-wide | `2xl:` |

**Persona breakpoints:**
- **Operator:** Mobile-first, tested at 375px (minimum) → 768px (fallback to desktop)
- **Builder:** Desktop-first, minimum 768px, primary at 1024px

---

## 7. COMPONENT SPECIFICATIONS

### 7.1 Button

**File:** `ui/button/Button.svelte`

**Variants:**

| Variant | Classes | Shadow | Hover | Active |
|---------|---------|--------|-------|--------|
| `default` | `bg-primary text-primary-foreground shadow-sm` | `shadow-sm` | `bg-primary/90` | `scale-0.98` |
| `primary-solid` | `bg-primary-solid` | `var(--shadow-primary-sm)` | Enhanced shadow | `scale-0.98` |
| `primary-gradient` | `bg-primary-gradient` | `var(--shadow-primary-md)` | `shadow-primary-lg + lift` | `scale-0.98` |
| `destructive` | `bg-destructive text-destructive-foreground shadow-sm` | `shadow-sm` | `bg-destructive/90` | `scale-0.98` |
| `success` | `bg-emerald-600 text-white shadow-sm` | `shadow-sm` | `bg-emerald-600/90` | `scale-0.98` |
| `outline` | `border border-border bg-transparent text-foreground` | — | `bg-muted` | `scale-0.98` |
| `secondary` | `bg-muted text-foreground` | — | `bg-muted/80` | `scale-0.98` |
| `ghost` | `text-foreground` | — | `bg-muted` | `scale-0.98` |
| `link` | `text-primary underline-offset-4` | — | `underline` | — |
| `elevated` | `bg-primary text-primary-foreground shadow-md` | `shadow-md` | `shadow-lg + bg-primary/90` | `scale-0.98` |
| `soft` | `bg-primary/10 text-primary` | — | `bg-primary/15` | `scale-0.98` |

**Sizes:**

| Size | Height | Padding | Rounded |
|------|--------|---------|---------|
| `default` | `h-10` | `px-4 py-2` | `rounded-md` |
| `sm` | `h-9` | `px-3` | `rounded-md` |
| `lg` | `h-11` | `px-8` | `rounded-md` |
| `icon` | `h-10 w-10` | center content | `rounded-md` |

**Base classes:**
```
inline-flex items-center justify-center whitespace-nowrap rounded-md text-sm font-medium 
ring-offset-background transition-all duration-150 
focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 
disabled:pointer-events-none disabled:opacity-50 active:scale-[0.98]
```

**Props:**
- `variant?: "default" | "primary-solid" | "primary-gradient" | "destructive" | "success" | "outline" | "secondary" | "ghost" | "link" | "elevated" | "soft"`
- `size?: "default" | "sm" | "lg" | "icon"`
- `loading?: boolean` — shows spinner, disables button
- `disabled?: boolean`

---

### 7.2 Card

**File:** `ui/card/Card.svelte`

**Props:**
- `interactive?: boolean` — `true` applies `.glass-card-hover` (lift effect), `false` applies `.glass-card` (static)
- `premium?: boolean` — adds 2px primary-tinted border + glow shadow

**Base classes:**
```
glass-card OR glass-card-hover
glass-border rounded-xl text-card-foreground
```

**Pattern usage:**
```svelte
<Card interactive data-testid="entity-card">
  <!-- Status bar -->
  <div class="h-0.5 w-full bg-success"></div>
  
  <!-- Content -->
  <div class="px-3.5 pt-3 pb-2.5">
    <!-- Avatar + title + badge row -->
    <div class="flex items-center gap-2.5">
      <Avatar size="md" ring />
      <div class="flex-1 min-w-0">
        <p class="text-[13px] font-medium truncate">Title</p>
        <p class="text-[11px] text-muted-foreground">Subtitle</p>
      </div>
      <Badge>Status</Badge>
    </div>
    
    <!-- Description -->
    <p class="mt-2 text-xs text-muted-foreground line-clamp-2">Body</p>
  </div>
  
  <!-- Footer (optional) -->
  <div class="border-t border-border/50 px-3.5 py-2 flex justify-end gap-1">
    <Button size="sm" variant="ghost">Action</Button>
  </div>
</Card>
```

---

### 7.3 Badge

**File:** `ui/badge/Badge.svelte`

**Variants:**

| Variant | Light Background | Dark Background | Text Color | Usage |
|---------|------------------|-----------------|------------|-------|
| `neutral` | `bg-muted` | — | `text-muted-foreground` | Neutral status |
| `primary` | `bg-primary/10` | `bg-primary/20` | `text-primary` | Active/ready |
| `success` | `bg-success/10` | `bg-success/20` | `text-success-a11y` | Success |
| `warning` | `bg-warning/10` | `bg-warning/20` | `text-warning-a11y` | Warning/caution |
| `danger` | `bg-destructive/10` | `bg-destructive/20` | `text-danger-a11y` | Error/failed |
| `info` | `bg-info/10` | `bg-info/20` | `text-info` | Info |
| `outline` | `transparent` | `transparent` | `text-foreground` | Outlined badge |
| `gradient-primary` | Gradient light → primary | Gradient dark → primary | `text-primary` | Premium variant |
| `gradient-success` | Gradient light → green | Gradient dark → green | `text-success-a11y` | Premium success |
| `gradient-warning` | Gradient light → amber | Gradient dark → amber | `text-warning-a11y` | Premium warning |
| `gradient-destructive` | Gradient light → red | Gradient dark → red | `text-danger-a11y` | Premium error |

**Sizes:**

| Size | Text | Padding | Gap |
|------|------|---------|-----|
| `sm` | `text-[10px]` | `px-2 py-0.5` | `gap-1` |
| `md` | `text-xs` | `px-2.5 py-0.5` | `gap-1.5` |
| `lg` | `text-sm` | `px-3 py-1` | `gap-1.5` |

**Base classes:**
```
inline-flex items-center rounded-full border border-transparent font-medium transition-colors
```

**Props:**
- `variant?: Variant` (see above)
- `size?: "sm" | "md" | "lg"`
- `icon?: Snippet` — optional leading icon
- `children?: Snippet` — label text

---

### 7.4 Input

**File:** `ui/input/Input.svelte`

**Base classes:**
```
flex h-10 w-full rounded-md border border-border bg-background px-3 py-2 text-sm
ring-offset-background transition-shadow duration-150
placeholder:text-muted-foreground
focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:border-primary
disabled:cursor-not-allowed disabled:opacity-50
```

**Props:**
- `value?: string` (bindable)
- `icon?: typeof Icon` — leading lucide icon
- `trailing?: Snippet` — trailing element (clear button, etc.)
- `disabled?: boolean`
- All HTML input attributes

**With icon layout:**
- Icon: `absolute left-3 top-1/2 -translate-y-1/2`
- Input: `pl-10` (when icon present)
- Trailing: `absolute right-2 top-1/2 -translate-y-1/2`

---

### 7.5 Dialog

**File:** `ui/dialog/Dialog.svelte`

**Sizes** (responsive, `min()` applied automatically):

| Size | Max Width | Responsive |
|------|-----------|-----------|
| `sm` | 440px | `w-[min(440px,90vw)]` |
| `md` | 560px | `w-[min(560px,90vw)]` |
| `lg` | 720px | `w-[min(720px,90vw)]` |
| `xl` | 920px | `w-[min(920px,90vw)]` |

**Animations:**
- Backdrop: `fade({ duration: 300 })`
- Dialog: `scale({ start: 0.97, duration: 300, easing: backOut })`
- Respects `prefers-reduced-motion`

**Accessibility:**
- `role="dialog"` + `aria-modal="true"` + `aria-label`
- Escape closes dialog
- Focus trap: Tab cycles through focusable elements
- First focusable element auto-focused

**Props:**
- `open: boolean` (required)
- `onclose: => void` (required)
- `size?: "sm" | "md" | "lg" | "xl"` (default: `"md"`)
- `title?: string` — optional header title
- `children?: Snippet`
- `data-testid?: string`

---

### 7.6 Toggle (Switch)

**File:** `ui/toggle/Toggle.svelte`

**Sizes:**

| Size | Track | Dot | Translate on | Spinner |
|------|-------|-----|--------------|---------|
| `sm` | `h-4 w-7` | `h-3 w-3` | `translate-x-3` | 10px |
| `default` | `h-5 w-9` | `h-4 w-4` | `translate-x-4` | 12px |

**Colors:**
- Unchecked: `bg-border` (muted)
- Checked: `bg-primary`
- Dot: `bg-white` (always light) unless loading

**Props:**
- `checked?: boolean` (bindable)
- `onchange?: (checked: boolean) => void`
- `size?: "sm" | "default"`
- `disabled?: boolean`
- `loading?: boolean` — shows spinner inside dot
- `aria-label?: string`

---

### 7.7 Avatar

**File:** `ui/avatar/Avatar.svelte`

**Deterministic color function:**

```typescript
function avatarHue(name: string): number {
  let sum = 0;
  for (let i = 0; i < name.length; i++) sum += name.charCodeAt(i);
  return sum % 360;
}
```

Then style with: `hsl({hue}, 60%, 48%)` background, `hsla({hue}, 60%, 38%, 0.3)` shadow

**Sizes:**

| Size | Dimensions | Text Size | Rounded | Class |
|------|-----------|-----------|---------|-------|
| `xs` | 6×6 | 10px | `rounded-md` | `h-6 w-6 text-[10px]` |
| `sm` | 8×8 | 12px | `rounded-lg` | `h-8 w-8 text-xs` |
| `md` | 10×10 | 14px | `rounded-lg` | `h-10 w-10 text-sm` |
| `lg` | 12×12 | 16px | `rounded-xl` | `h-12 w-12 text-base` |
| `xl` | 16×16 | 18px | `rounded-2xl` | `h-16 w-16 text-lg` |

**Props:**
- `name: string` — derives color + initials
- `size?: AvatarSize` (default: `"md"`)
- `src?: string | null` — optional image URL
- `fallback?: string | null` — override display text
- `ring?: boolean` — adds border ring effect
- `class?: string`

---

## 8. MOTION & ANIMATION

### 8.1 Duration Tokens

```typescript
// in CSS and JS (@lib/design/motion.ts)
--motion-fast: 120ms;    // Micro-interactions
--motion-base: 200ms;    // Default transitions
--motion-slow: 320ms;    // Large surfaces, overlays
```

In Tailwind: `duration-fast`, `duration-base`, `duration-slow`

### 8.2 Easing Curves

```typescript
// Tailwind registered (--ease-apple in app.css)
--ease-apple: cubic-bezier(0.2, 0, 0, 1);      // Decelerate (main)
--ease-standard: cubic-bezier(0.4, 0, 0.2, 1); // Standard ease-in-out
--ease-accel: cubic-bezier(0.4, 0, 1, 1);      // Accelerate (exits)
--ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1); // Elastic bounce
```

### 8.3 Spring Presets

From `motion.ts`:

```typescript
spring.default = { stiffness: 0.22, damping: 0.24 };  // Stiff, low bounce
spring.gentle = { stiffness: 0.12, damping: 0.32 };   // Slow, soft
spring.snappy = { stiffness: 0.38, damping: 0.28 };   // Punchy, short
```

Physical constants (for documentation):
```typescript
springPhysics.default = { stiffness: 220, damping: 24, mass: 1 };
springPhysics.gentle = { stiffness: 120, damping: 32, mass: 1 };
springPhysics.snappy = { stiffness: 380, damping: 28, mass: 0.8 };
```

### 8.4 CSS Animations

All defined in `app.css` `@layer utilities`:

| Animation | Keyframes | Duration | Use Case |
|-----------|-----------|----------|----------|
| `animate-fade-in` | `opacity 0→1` | 200ms (apple easing) | Page entrance |
| `animate-slide-up` | `opacity + translateY(8px→0)` | 300ms | Entrance |
| `animate-scale-in` | `opacity + scale(0.96→1)` | 250ms | Entrance |
| `animate-slide-in-right` | `translateX(100%→0)` | 300ms | Slide in from right |
| `animate-glow-pulse` | Box-shadow pulse | 2s infinite | Pulsing glow |
| `animate-shimmer-slide` | Gradient translation | 1.4s infinite | Skeleton loading |
| `animate-spinner-rotate` | `rotate(0→360deg)` | 700ms infinite | Loading spinner |

### 8.5 Transition Examples

```svelte
<!-- Page entrance -->
{#key $currentRoute}
  <div transition:fade={{ duration: 150 }}>
    <Page />
  </div>
{/key}

<!-- List item entrance with stagger -->
{#each items as item, i (item.id)}
  <div in:fly={{ y: 4, duration: 200, delay: i * 30 }}>
    <ItemCard />
  </div>
{/each}

<!-- Dialog scale -->
<div transition:scale={{ start: 0.97, duration: 300, easing: backOut }} />

<!-- Sheet slide from right -->
<div transition:fly={{ x: 400, duration: 250, easing: cubicOut }} />
```

---

## 9. Z-INDEX STACKING

```css
--z-backdrop: 40;  /* Modal backdrop (shared with toast) */
--z-toast: 40;     /* Toast notifications */
--z-overlay: 50;   /* Modal/sheet dialogs */
--z-tooltip: 70;   /* Tooltips (on top of everything) */
```

---

## 10. RESPONSIVE PATTERNS

### 10.1 Page Container

```svelte
<div class="mx-auto w-full max-w-6xl px-4 sm:px-6 lg:px-8">
  <!-- Page content -->
</div>
```

### 10.2 Responsive Grid

```svelte
<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
  {#each items as item}
    <Card />
  {/each}
</div>
```

### 10.3 Header

```svelte
<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
  <div>
    <h1 class="text-2xl font-semibold tracking-tight">{title}</h1>
    <p class="text-xs text-muted-foreground mt-1">{subtitle}</p>
  </div>
  <Button size="sm">Action</Button>
</div>
```

---

## 11. COMPONENT INVENTORY

### Primitive UI Components

All in `lib/components/ui/`:

| Component | File | States | Variants | Accessibility |
|-----------|------|--------|----------|----------------|
| Button | `button/Button.svelte` | default, hover, active, disabled, loading | 11 (see §7.1) | `aria-busy`, keyboard accessible |
| Card | `card/Card.svelte` | static, interactive (hover lift) | — | Semantic div |
| Badge | `badge/Badge.svelte` | — | 14 (see §7.3) | `aria-hidden` on icon |
| Input | `input/Input.svelte` | default, focus, disabled, error | text, email, password, etc. | Focus ring, label-able |
| Textarea | `textarea/Textarea.svelte` | default, focus, disabled | — | Focus ring, label-able |
| Checkbox | `checkbox/Checkbox.svelte` | unchecked, checked, indeterminate, disabled | — | `role="checkbox"`, `aria-checked` |
| Toggle | `toggle/Toggle.svelte` | off, on, disabled, loading | 2 sizes | `role="switch"`, `aria-checked` |
| Radio | `radio/RadioGroup.svelte` + `RadioItem.svelte` | unselected, selected, disabled | — | `role="radio"`, `aria-checked` |
| Select | `select/Select.svelte` | default, open, selected, disabled | — | Native `<select>` styling |
| Dialog | `dialog/Dialog.svelte` | closed, open | 4 sizes | Focus trap, keyboard (Escape) |
| Sheet | `sheet/Sheet.svelte` | closed, open (left/right) | — | Focus trap, keyboard |
| Toast | `toast/Toast.svelte` | success, error, info | — | Auto-dismiss 4s |
| Popover | `popover/Popover.svelte` | closed, open | — | Click-away dismiss |
| Separator | `separator/Separator.svelte` | horizontal, vertical | — | 1px borders |
| Skeleton | `skeleton/Skeleton.svelte` | — | — | Shimmer animation |
| Spinner | `progress/Spinner.svelte` | — | — | Rotate animation |
| ProgressBar | `progress/ProgressBar.svelte` | determinate, indeterminate | — | A11y-labeled |
| Avatar | `avatar/Avatar.svelte` | — | 5 sizes | `aria-label` optional |

### Layout Components

| Component | File | Purpose |
|-----------|------|---------|
| Sidebar | `layout/Sidebar.svelte` | App navigation sidebar |
| Main | `layout/Main.svelte` | Main content area |
| SkipToContent | `layout/SkipToContent.svelte` | A11y skip link |

### Complex Components

| Component | File | Use Case |
|-----------|------|----------|
| AgentCard | `agents/AgentCard.svelte` | Agent list item with actions |
| AgentDetail | `agents/AgentDetail.svelte` | Agent detail view |
| ChatConversation | `chat/ChatConversation.svelte` | Message thread |
| ChatMessageBubble | `chat/ChatMessageBubble.svelte` | Individual message |
| CommandPalette | `command-palette/CommandPalette.svelte` | Global search palette |
| AutomationCard | `automations/AutomationCard.svelte` | Automation item |
| ApprovalCard | `chat/ApprovalCardV2.svelte` | HITL approval UI |

---

## 12. ICON SYSTEM

**Library:** `lucide-svelte` v0.460.1

**Icon usage:**
```svelte
<script>
  import { Play, Square, MessageSquare, Trash2, RefreshCw, Zap, Check, AlertTriangle, Clock } from "lucide-svelte";
</script>

<Button>
  <Play size={16} class="mr-2" />
  Start
</Button>
```

**Icon sizing conventions:**
- Inline in text: `size={14}` to `size={16}`
- Buttons: `size={16}` (default)
- Large buttons: `size={20}`
- Badges: `size={8}` to `size={12}`
- Sidebars: `size={20}`

**Status icons:**
- Success: `CheckCircle` (green)
- Error: `XCircle` (red)
- Warning: `AlertTriangle` (amber)
- Loading: `Loader2` with `animate-spin`
- Info: `Info` (blue)

---

## 13. DARK MODE

### 13.1 How It Works

Dark mode is applied via `.dark` class on `<html>` or `<body>`. Resolved automatically for all CSS variables and Tailwind utilities.

```typescript
// from $lib/stores/theme.ts
export function initTheme() {
  const isDark = localStorage.getItem("theme") === "dark" || 
                 window.matchMedia("(prefers-color-scheme: dark)").matches;
  if (isDark) document.documentElement.classList.add("dark");
}
```

### 13.2 Dark-specific Classes

```svelte
<!-- Light only -->
<div class="bg-red-50 dark:hidden">Light</div>

<!-- Dark only -->
<div class="hidden dark:block dark:bg-red-950">Dark</div>

<!-- Different per mode -->
<div class="text-foreground dark:text-foreground">Auto</div>
```

### 13.3 Overriding Colors in Dark

```svelte
<!-- Using semantic tokens (preferred) -->
<div class="bg-muted dark:bg-muted">Auto</div>

<!-- Direct variant (for one-offs) -->
<div class="bg-red-50 dark:bg-red-950">Custom</div>
```

---

## 14. INTERNATIONALIZATION (i18n)

**Library:** `svelte-i18n` v4.0.1

**Files:**
- `src/lib/i18n/en.json` (English, reference)
- `src/lib/i18n/fr.json` (French)

**Usage:**
```svelte
<script>
  import { t } from "svelte-i18n";
</script>

<h1>{$t("agents.title")}</h1>
<p>{$t("agents.description", { values: { count: items.length } })}</p>
```

**Naming convention:**
```
{page}.{section}.{element}
```

Example structure:
```json
{
  "agents.title": "Agents",
  "agents.description": "You have {count} agent(s)",
  "agents.empty.title": "No agents configured",
  "common.actions.delete": "Delete",
  "common.actions.cancel": "Cancel"
}
```

---

## 15. ACCESSIBILITY (WCAG AA)

### 15.1 Focus Visible

All interactive elements receive:
```css
focus-visible:outline-none 
focus-visible:ring-2 
focus-visible:ring-ring/60 
focus-visible:ring-offset-2
```

### 15.2 ARIA Roles

| Element | Role | Required Attributes |
|---------|------|---|
| Checkbox | `role="checkbox"` | `aria-checked` |
| Toggle/Switch | `role="switch"` | `aria-checked` |
| Dialog | `role="dialog"` | `aria-modal="true"`, `aria-label` |
| Sheet | `role="dialog"` | `aria-modal="true"` |
| RadioGroup | `role="radiogroup"` | — |
| Button (loading) | — | `aria-busy="true"` |

### 15.3 Keyboard Navigation

- **Tab** — cycle focus through interactive elements
- **Shift+Tab** — reverse cycle
- **Escape** — close Dialog/Sheet
- **Space/Enter** — toggle Checkbox/Toggle/Button
- **Arrow keys** — navigate RadioGroup/Combobox

### 15.4 Contrast Ratios

All text meets **≥4.5:1** on backgrounds:
- Primary text on card: foreground on surface-1 ✓
- Secondary text: muted-foreground on background ✓
- Success/warning/danger text use darker token variants (not the bg color)

### 15.5 Semantic HTML

```svelte
<!-- Good -->
<h1>Main Title</h1>
<h2>Section</h2>
<button type="button">Action</button>
<input type="text" aria-label="Search" />

<!-- Bad -->
<div class="text-2xl font-bold">Main Title</div>
<span role="button">Action</span>
```

---

## 16. TESTING & DATA ATTRIBUTES

**Convention:** `data-testid="{page}-{action}-{entity}"`

Examples:
```
triggers-page              ← page container
triggers-create-btn        ← CTA button
trigger-card-{id}          ← individual item
trigger-delete-confirm     ← confirmation button
agent-card                 ← card component
agent-avatar               ← sub-element
dialog-close               ← dialog close button
```

---

## 17. PERFORMANCE NOTES

### 17.1 Motion Preferences

Always respect `prefers-reduced-motion`:

```typescript
import { prefersReducedMotion } from "$lib/design/motion";

if (prefersReducedMotion()) {
  // Use fade instead of scale
  // Set duration to 0
  // Skip spring animations
}
```

### 17.2 Image & Asset Optimization

- Icons: inline lucide-svelte (SVG)
- Backgrounds: CSS gradients (no images)
- Avatars: generated CSS `hsl()` gradients (no images)

---

## 18. EXTERNAL DEPENDENCIES

### Production Dependencies

```json
{
  "@fontsource/inter": "^5.2.8",
  "@tauri-apps/api": "^2.0.0",
  "@tauri-apps/plugin-dialog": "^2.0.0",
  "@tauri-apps/plugin-notification": "^2.0.0",
  "bits-ui": "^1.0.0-next.50",
  "clsx": "^2.1.1",
  "@tanstack/svelte-virtual": "^3.10.9",
  "dompurify": "^3.3.3",
  "highlight.js": "^11.11.1",
  "lucide-svelte": "^0.460.1",
  "marked": "^17.0.5",
  "shiki": "^1.29.2",
  "svelte-i18n": "^4.0.1",
  "tailwind-merge": "^2.6.0"
}
```

### Dev Dependencies

```json
{
  "@sveltejs/vite-plugin-svelte": "^5.0.0",
  "@types/dompurify": "^3.0.5",
  "@playwright/test": "^1.49.1",
  "@vitest/coverage-v8": "^2.1.9",
  "autoprefixer": "^10.4.20",
  "postcss": "^8.4.49",
  "svelte": "^5.0.0",
  "svelte-check": "^4.0.0",
  "tailwindcss": "^3.4.17",
  "typescript": "^5.6.0",
  "vite": "^6.0.0",
  "vitest": "^2.1.9"
}
```

---

## 19. QUICK REFERENCE

### Color Quick Links

| Use | Variable |
|-----|----------|
| Primary button | `bg-primary text-primary-foreground` |
| Secondary button | `bg-muted text-foreground` |
| Success | `bg-success/10 text-success-a11y` or `badge variant="success"` |
| Warning | `bg-warning/10 text-warning-a11y` or `badge variant="warning"` |
| Error | `bg-destructive/10 text-danger-a11y` or `badge variant="danger"` |
| Borders | `border border-border` |
| Glass | `glass-card`, `glass-panel`, `glass-surface` |

### Spacing Quick Links

| Use | Tailwind |
|-----|----------|
| Tight gap | `gap-1` to `gap-2` |
| Standard gap | `gap-3` to `gap-4` |
| Loose gap | `gap-6` to `gap-8` |
| Card padding | `p-3.5` (px-3.5 py-3) |
| Section padding | `p-4 sm:p-6 lg:p-8` |

### Typography Quick Links

| Use | Tailwind |
|-----|----------|
| Page title | `text-2xl font-semibold tracking-tight` |
| Section header | `text-sm font-medium uppercase tracking-wider` |
| Card title | `text-[13px] font-medium` |
| Body | `text-xs` or `text-[13px]` |
| Helper text | `text-[10px] text-muted-foreground` |

---

## 20. FILES REFERENCE

| File | Purpose |
|------|---------|
| `crates/apollia-desktop/ui/src/app.css` | All CSS variables, glass layers, animations, global styles |
| `crates/apollia-desktop/ui/tailwind.config.ts` | Tailwind config, breakpoints, color aliases, extended utilities |
| `crates/apollia-desktop/ui/src/lib/design/tokens.ts` | TypeScript design token constants (elevation, shadows, gradients) |
| `crates/apollia-desktop/ui/src/lib/design/motion.ts` | Motion presets, easing, spring constants, reduced-motion hook |
| `crates/apollia-desktop/ui/src/lib/design/breakpoints.md` | Responsive design guidelines |
| `crates/apollia-desktop/ui/src/lib/components/ui/` | All primitive components |
| `crates/apollia-desktop/ui/src/lib/i18n/en.json` | English translation keys |
| `crates/apollia-desktop/ui/src/lib/i18n/fr.json` | French translation keys |
| `docs/adr/ADR-077-design-tokens-v2.md` | Full design decision record |

---

**End of Comprehensive Design System Document**
