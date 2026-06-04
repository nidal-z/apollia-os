# Apollia Design System V3 - Layout & UX Patterns

> Référence canonique pour le frontend Apollia OS (Tauri v2 + Svelte 5 + Tailwind 3.4).
> Établie à l'issue de la refonte UX/UI mai 2026 (Tasks → Agents → Projets → Connecteurs).
> Toute nouvelle page doit s'aligner sur les patterns ci-dessous, sauf déviation explicite
> documentée dans un commentaire en tête de fichier.

---

## 1. Philosophie

**Une seule grammaire visuelle, trois familles de pages.** Chaque route Apollia tombe dans
exactement une de ces trois familles, jamais à cheval. Les primitives canon
(`$lib/components/operator/*`, `$lib/components/ui/*`) sont les briques uniques ; les
composants ad-hoc dans `src/components/*` doivent les composer, pas les réinventer.

**Le détail ne s'ouvre pas, il se révèle.** Plus de Sheet/drawer pour les contextes
métier - le détail vit dans le right pane via tabs. Les Sheets sont réservés aux
contextes ponctuels : wizards d'install, dialogues d'ajout, prévisualisation.

**Sidebar + Detail avec tabs = pattern primaire** pour toute page qui présente une
collection d'entités gérables (Projets, Assistants, Tâches, Connecteurs).

---

## 2. Les 3 familles de layout

### Famille A - **Sidebar + Detail tabs** (pattern primaire)

Pour les pages qui listent et gèrent des entités persistantes. Le détail est toujours
visible (premier item auto-sélectionné). Onglets dans le right pane.

**Pages :** Projets · Assistants · Tâches (mode split) · Connecteurs · Mémoire (proche).

**Structure :**

```
[breadcrumb global Apollia / <Page>]
┌─ aside (240–300px) ──┬─ section (flex-1) ──────────────────────────┐
│ MES <ENTITÉS> · N    │ [icône] [Badge statut] [meta date]          │
│ [filter chips]       │ h2 Nom de l'entité                          │
│ [search optional]    │ description…   [Action²][Action¹ primary]   │
│ ─────────────────────├────────────────────────────────────────────┤
│ ▎ Item A (active)    │ Tab 1 | Tab 2 | Tab 3 | …                  │
│   subtitle           │ ─────                                       │
│ ▎ Item B             │ contenu de l'onglet actif                   │
│   subtitle           │ (Cards space-y-4 dans max-w-3xl)            │
│ …                    │                                             │
│ ─────────────────────│                                             │
│ + Action ajout       │                                             │
└──────────────────────┴─────────────────────────────────────────────┘
```

**Aside (`<aside class="w-[240..300px] shrink-0 border-r border-border flex flex-col bg-background">`) :**

| Section | Contenu | Classes clés |
|---|---|---|
| Header | `MES ENTITÉS · N` en `font-mono text-[10.5px] uppercase tracking-[1.2px]` muted | `px-4 pt-4 pb-2.5` |
| Search (optionnel) | `<Input unstyled>` wrappé dans `flex items-center gap-2 px-2 py-1.5 rounded-md bg-surface-1 border border-border` | - |
| Filter chips (optionnel) | Pills `rounded-full border px-2 py-0.5 text-[10.5px] font-medium` - actif : `border-primary/40 bg-primary/10 text-primary` | `flex flex-wrap gap-1` |
| Item rows | Voir "Sidebar list rows" ci-dessous | `flex-1 overflow-auto px-2.5 pb-3` |
| Footer CTA | `<Button variant="outline" size="sm" class="w-full justify-center">` | `px-3 py-2 border-t border-border` |

**Largeurs canoniques :**
- `w-[240px]` - Projets (titres courts, liste dense)
- `w-[280px]` - Tasks (titres tronqués + agent name + badge statut)
- `w-[300px]` - Connecteurs (deux groupes : natifs + MCP)
- `w-[320px]` - Assistants (titres + description + status indicators)

### Famille B - **PageHeader + content flat** (pattern legacy/secondaire)

Pour les pages "tableau de bord" / "indicateurs" sans entité primaire à éditer.

**Pages :** Dashboard · Mon travail (Tasks mode list) · Inbox · Notifications · Mémoire (route /memory standalone, en cohabitation avec son détail-Sheet existant).

**Structure :**

```
[breadcrumb global]
[PageHeader kicker + title + subtitle + actions]
[Body : grids, tables, sections]
```

`<PageHeader>` (kicker `text-[10.5px] uppercase tracking-[1.2px] font-mono`, title `text-[24..28px] font-semibold`, subtitle `text-[12.5px] muted`) - depuis `$lib/components/operator/PageHeader`.

### Famille C - **Left rail + content** (pattern Settings only)

Pour les pages avec sous-navigation verticale par catégorie. Ne pas étendre à d'autres
contextes sans nouveau primitive `VerticalRailNav`.

**Pages :** Settings (CLUSTER_PERSONALIZATION / CLUSTER_AI / CLUSTER_SYSTEM / CLUSTER_DANGER).

---

## 3. Sidebar list rows - spec stricte

Le composant le plus répliqué de l'app. Toute liste de sidebar (Projets, Assistants,
Tâches split, Connecteurs) suit **exactement** cette structure :

```svelte
<Button variant="ghost" size="auto"
  type="button"
  onclick={...}
  class="w-full text-left flex items-start gap-2.5 px-2.5 py-2.5 rounded-lg mb-0.5 border-0 transition-colors {isActive
    ? 'bg-primary/10'
    : 'bg-transparent hover:bg-muted/40'}"
>
  <!-- Accent bar (2px wide, full row height) -->
  <div
    class="w-0.5 self-stretch rounded-sm shrink-0 my-0.5"
    style="background: {accentColor};"
  ></div>

  <div class="flex-1 min-w-0">
    <!-- Title row (optional inline dot/icon after name) -->
    <div class="flex items-center gap-1.5 min-w-0">
      <span class="text-[12.5px] truncate text-foreground" style:font-weight={isActive ? 600 : 500}>
        {title}
      </span>
      {#if running}
        <StatusDot color="hsl(var(--success))" glow size={5} />
      {/if}
    </div>
    <!-- Subtitle -->
    <div class="text-[10.5px] text-muted-foreground mt-0.5 truncate">{subtitle}</div>
  </div>

  <!-- Optional trailing badge -->
  <Badge size="sm" variant="..." class="shrink-0 text-[8px] px-1 py-0 leading-[1.4]">
    {label}
  </Badge>
</Button>
```

**Règles non-négociables :**
- **Bouton wrapper** : `variant="ghost" size="auto"` (jamais `size="sm"` qui force `h-9`).
- **Accent bar** : `w-0.5` (2px), pas plus large. Couleur sémantique selon le statut.
- **Title** : `text-[12.5px]` toujours. `font-weight: 600` quand actif (style:font-weight, pas une classe).
- **Subtitle** : `text-[10.5px] text-muted-foreground mt-0.5 truncate`.
- **Active state** : `bg-primary/10` (ne pas surcharger avec border).
- **Trailing badge** : si présent, `text-[8px] px-1 py-0 leading-[1.4]` (rester discret).
- **Status dot inline** : `size={5}` (5px), glow uniquement quand `active`.

**Couleurs d'accent canoniques :**
| Statut | Couleur |
|---|---|
| Active / Running | `hsl(var(--success))` |
| Error / Failed | `hsl(var(--destructive))` |
| Warning / Approval | `hsl(var(--warning))` |
| Processing / Working | `hsl(var(--primary))` |
| Idle / Default | `hsl(var(--muted-foreground))` |

Sections au sein de la sidebar : header `section-meta mt-N mb-1.5 px-2 text-[10px] tracking-[1.4px]`.

---

## 4. Right pane - header + tabs

Quand un item est sélectionné dans la sidebar, le right pane affiche :

```svelte
<section class="flex-1 flex flex-col min-w-0 overflow-hidden bg-background">
  <!-- ─── Header ─────────────────────────────────────────────── -->
  <div class="px-8 pt-6 pb-4 border-b border-border/60">
    <div class="flex items-start gap-3.5">
      <!-- Logo / Avatar (12 w/h, lg rounded, gradient ou color tile) -->
      <div class="w-12 h-12 shrink-0 rounded-lg inline-flex items-center justify-center" style="...">
        <Icon size={18} color="white" />
      </div>

      <!-- Title block -->
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-2 mb-1">
          <Badge variant={statusVariant} size="sm">
            {#snippet icon()}<StatusDot color={statusColor} glow={isActive} />{/snippet}
            {statusLabel}
          </Badge>
          <span class="text-[10.5px] text-muted-foreground">{relativeDate}</span>
        </div>
        <h2 class="m-0 text-foreground"
          style="font-size: 22px; font-weight: 600; letter-spacing: -0.3px; line-height: 1.2;">
          {name}
        </h2>
        <p class="mt-1 max-w-[600px] text-[12.5px] leading-[1.5] text-muted-foreground">
          {description}
        </p>
      </div>

      <!-- Trailing actions -->
      <div class="flex shrink-0 gap-1.5">
        <Button variant="outline" size="sm">Action secondaire</Button>
        <Button variant="primary-solid" size="sm">Action primaire</Button>
      </div>
    </div>
  </div>

  <!-- ─── Tabs ──────────────────────────────────────────────── -->
  <div class="px-8 pt-3.5">
    <TabBar variant="underline" testidPrefix="..." items={...} activeTab={...} ontabchange={...} />
  </div>

  <!-- ─── Tab content ───────────────────────────────────────── -->
  <div class="flex-1 overflow-auto px-8 py-5">
    {#if activeTab === "tab1"}
      <div class="space-y-4 max-w-3xl"> <!-- always max-w-3xl + space-y-4 -->
        <Card class="p-[14px_16px]">
          <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 mb-2">
            Section header
          </div>
          ...
        </Card>
      </div>
    {/if}
  </div>
</section>
```

**Règles non-négociables :**
- **Pas de bordure colorée supérieure** (status accent strip `<div class="h-0.5 w-full bg-...">`) - proscrite (régression catastrophique).
- **Pas de PageHeader top-level** quand le right pane affiche son propre header.
- **Title** : `font-size: 22px; font-weight: 600; letter-spacing: -0.3px; line-height: 1.2` en style inline (pas une classe Tailwind faute de token équivalent).
- **TabBar variant** : toujours `"underline"` dans ce contexte (jamais `"pill"`).
- **Tab content** : wrap dans `<div class="space-y-4 max-w-3xl">` ; cards utilisent `Card class="p-[14px_16px]"` (ou `p-[16px_18px]` pour les "stat" cards).
- **Section header dans card** : `<div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 mb-2">`.

---

## 5. Filter chips (statuts / catégories)

**Toujours** le pattern raw-button rounded-full :

```svelte
<button
  type="button"
  role="tab"
  aria-selected={isActive}
  onclick={(e) => { activeFilter = f.key; (e.currentTarget as HTMLButtonElement).blur(); }}
  class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11.5px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/60 {isActive
    ? 'border-primary/40 bg-primary/10 text-primary'
    : 'border-border bg-transparent text-muted-foreground hover:text-foreground'}"
>
  <StatusDot color={f.color} glow={isActive && f.key === 'active'} />
  {f.label}
  <span class="tabular-nums {isActive ? 'text-primary/80' : 'text-muted-foreground/60'}">
    {f.count}
  </span>
</button>
```

**Règles :**
- **Le `.blur()` après click** est obligatoire - sans ça, le focus ring persiste et fait croire à un état actif fantôme (régression observée).
- **Variante sidebar (compact)** : `px-2 py-0.5 text-[10.5px]`.
- **Variante list/grid (full)** : `px-2.5 py-1 text-[11.5px]`.
- **Glow** : uniquement quand `isActive` ET la key est sémantiquement "en cours" (`active`, `running`).

**NE PAS utiliser** : `<Badge variant="primary" outline={false}>` dans un wrapper `<Button>` - le double layer crée un focus ring fantôme.

---

## 6. Sheets - quand et comment

`<Sheet>` est réservé aux contextes ponctuels :
- **Wizards** (install package, OAuth flow, connector wizard) - multi-step.
- **Catalogues / dialogues d'ajout** (catalogue MCP, etc.) - discovery + sélection.
- **Préviews** (tool schema panel, memory entry sheet) - read-only ou édition légère.

**Composition canonique** (depuis `$lib/components/ui/sheet`) :

```svelte
<Sheet open={open} onclose={onclose} class="w-full sm:max-w-[760..1100px]">
  <SheetHeader
    title="Titre du Sheet"
    subtitle="Subtitle optionnel."
    onclose={onclose}
  >
    {#snippet leading()}<Icon size={16} />{/snippet}
    {#snippet actions()}<Button size="sm" variant="outline">Action</Button>{/snippet}
  </SheetHeader>
  <SheetContent padding="default|compact|flush">
    ...
  </SheetContent>
  <SheetFooter>
    <Button variant="outline" size="sm">Annuler</Button>
    <Button variant="primary-solid" size="sm">Confirmer</Button>
  </SheetFooter>
</Sheet>
```

**Sheets internes à un Sheet (catalogue → wizard)** : autorisé, le wizard remplace
visuellement le catalogue. Au "complete" du wizard, fermer les deux et revenir à la
page parente avec la nouvelle entité sélectionnée.

**Anti-pattern proscrit** : utiliser `<Sheet>` pour le détail d'une entité éditable.
Ces contextes doivent vivre dans le right pane (famille A) avec tabs.

---

## 7. Primitives canoniques - inventaire rapide

### Layout & shell
- `<PageHeader kicker title subtitle actions>` - famille B uniquement.
- `<PageLayout>` - wrapper centré `max-w-6xl` + PageHeader intégré.
- `<SectionTitle>` - section header dense (`uppercase tracking-[1.4px] text-[10.5px]`).
- `<EmptyState icon title desc action tone="primary|neutral|success|warning">` - états vides.

### Forms
- `<Input unstyled?>` - input texte. `unstyled` quand wrappé dans un container avec bordure custom (cf. sidebar searches).
- `<Select>`, `<Textarea>`, `<Checkbox>`, `<Toggle>` - natifs stylés.
- `<FormField id label labelClass hint error required optional optionalLabel data-testid>` - wrapper canonique label + control + hint/error. Toujours utilisé pour les formulaires.

### Display
- `<Card class="p-[14px_16px]">` - surface contenu primaire. Hérite `bg-card border border-border rounded-xl`.
- `<Badge variant size outline>` - tag/statut. `size="sm"` pour l'inline, `size="md"` pour les chips standalone.
- `<StatusDot color glow size={5|6|7}>` - point d'état coloré.
- `<Avatar name fallback size="sm|md|lg" ring>` - avatar utilisateur/agent.
- `<Spinner size={11|14|24}>` - loader.
- `<Skeleton class="h-N w-N">` - placeholder loading.
- `<Banner variant surface="edge|card">` - bannière info/warning/error.
- `<Separator variant="default|inline">` - séparateur visuel.

### Navigation
- `<TabBar variant="underline|pill" items activeTab ontabchange testidPrefix>` - onglets. **Underline** dans le right pane. **Pill** rare (segmented control compact).
- `<Breadcrumbs items>` - global app shell (déjà câblé).

### Actions / Overlays
- `<Button variant size>` - voir spec ci-dessous.
- `<ActionMenu items|body triggerSlot triggerLabel align side>` - kebab menus.
- `<Sheet>` + `<SheetHeader>` + `<SheetContent>` + `<SheetFooter>` - drawer canonique.
- `<Dialog>` + `<DialogFooter>` - modal.
- `<ConfirmDialog>` - confirmation destructive standard.
- `<Popover trigger content>` - popovers (bits-ui sous le capot).
- `<Toast>` via `addToast(message, variant)` - notifications éphémères.

### Tabular
- `<DataTable data columns rowKey emptyLabel>` - table générique. **Ne supporte pas les rows expansibles** - pour les tables à dépliage (AuditTrail), garder une table inline.

---

## 8. Button variants - quand utiliser quoi

| Variant | Usage |
|---|---|
| `primary-solid` | Action primaire d'un contexte (header CTA, footer confirm). **Une seule par vue/section.** |
| `outline` | Action secondaire. Refresh, cancel, navigation latérale. |
| `ghost` | Wrapper pour rows cliquables (sidebar items, table rows interactives). Pas de fond, juste hover. |
| `destructive` | Disconnect, delete, supprimer. Toujours derrière une confirmation inline ou ConfirmDialog. |
| `success` | Validation explicite (rare - Badge success suffit souvent). |
| `link` | Actions de navigation inline dans du texte. |
| `secondary` | Action neutre (rare). |

**Sizes :**
| Size | Hauteur | Usage |
|---|---|---|
| `default` | `h-10` | CTA standard. |
| `sm` | `h-9` | Header actions, dense forms. |
| `lg` | `h-11` | Hero CTAs (rare). |
| `icon` | `h-10 w-10` | Bouton icône carré standard. |
| `icon-sm` | `h-7 w-7` | Bouton icône compact (toggle dans une row). |
| `auto` | aucune hauteur fixe (px-3 py-2) | Wrapper pour multi-line content (rows sidebar, cards cliquables). |

**Règle d'or** : si tu wrappes un layout multi-ligne dans un `<Button>`, c'est **toujours** `size="auto"`. Ne jamais utiliser `size="sm"` (h-9) sur une row de 3 lignes - elle sera écrasée.

**Hover-revealed buttons** (toggle Play/Stop dans une row) : pas de `hover:bg-N`. Utiliser :
```
class="text-muted-foreground transition-opacity hover:text-foreground hover:bg-transparent {active
  ? 'opacity-100'
  : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100'}"
```
Le bouton doit "fondre" dans la row, pas avoir son propre fond.

---

## 9. Typographie & spacing

### Échelle textuelle (text-[Npx])

| Px | Usage |
|---|---|
| `text-[9px]–[10.5px]` | Mono caps, badges, timestamps, meta. |
| `text-[10.5px]` | Subtitles dans rows sidebar, badges count. |
| `text-[11px]` | Description body dans cards. |
| `text-[11.5px]` | Body dense, helper text, badges medium. |
| `text-[12px]` | Card titles, table cell body. |
| `text-[12.5px]` | Row titles primaires, body forms. |
| `text-[13px]` | Memory entry keys, list items proéminents. |
| `text-sm` (14px) | Body standard. |
| `text-base` (16px) | h3 sections. |
| `22px` (style inline) | h2 titre right pane. |
| `24px` (style inline) | h2 titre détail (Projets). |

**Règle** : préférer `text-[N.5px]` arbitraires aux classes Tailwind (text-xs/sm) quand
on veut un ajustement fin. Garde la gamme cohérente avec les valeurs ci-dessus.

### Spacing

| Pattern | Usage |
|---|---|
| `space-y-4 max-w-3xl` | Tab content body. |
| `p-[14px_16px]` | Card content card. |
| `p-[16px_18px]` | Stat card / overview card. |
| `px-8 pt-6 pb-4` | Right pane header. |
| `px-8 pt-3.5` | Right pane TabBar wrapper. |
| `px-8 py-5` | Right pane tab content wrapper. |
| `px-4 pt-4 pb-2.5` | Sidebar header. |
| `px-2.5 py-2.5` | Sidebar list row. |
| `px-3 py-2` | Sidebar footer CTA wrapper. |
| `gap-2.5` | Espacement icon+text horizontal standard. |

### Couleurs sémantiques (tokens CSS ADR-021)

| Token | Usage |
|---|---|
| `--foreground` | Text primary. |
| `--muted-foreground` | Text secondary. |
| `--primary` | Action primary, active selection (bg-primary/10). |
| `--success` | Statut actif/réussi. |
| `--destructive` | Erreur, destructive action. |
| `--warning` | Approbation, dégradé. |
| `--info` | Information, statut secondaire. |
| `--border` | Bordures standard. |
| `--surface-1` | Surface légèrement contrastée (search box bg, sidebar). |
| `--card` | Surface card. |
| `--background` | Surface page principale. |

**Bordures hairline** : utiliser `border-border/40` pour les séparateurs très subtils dans
les cards, `border-border/60` pour les séparateurs forts (header bottom, table headers).

---

## 10. Anti-patterns explicitement proscrits

À ne **jamais** introduire (régressions identifiées) :

| Anti-pattern | Pourquoi |
|---|---|
| Status accent strip `<div class="h-0.5 w-full bg-...">` en haut du right pane | "Catastrophique" - écrase visuellement le contenu. |
| PageHeader full-width au-dessus d'un layout split (sidebar + detail) | Écrase la sidebar et bloque le pattern "header dans le right pane". |
| `<Badge>` dans un `<Button>` pour les filter chips | Génère un focus ring fantôme. Utiliser raw button rounded-full. |
| `size="sm"` (h-9) sur un Button wrappant une row multi-ligne | Force h-9 (36px) - le contenu est écrasé. Utiliser `size="auto"`. |
| Sheet pour le détail d'une entité éditable | Doit vivre dans le right pane avec tabs. |
| Badge "Système" / "Libre" / catégorie sur chaque row de liste | "Bas de gamme" - supprime du noise visuel sans valeur. |
| Liserai (accent bar) `w-1` (4px) ou plus | Trop large. Toujours `w-0.5` (2px). |
| Badge trailing `text-[9px] px-1.5` ou plus | Trop gros pour une sidebar row. `text-[8px] px-1 py-0 leading-[1.4]`. |
| Boutons toggle (Play/Stop) avec leur propre `hover:bg-N` | "Hors de la row" - utiliser hover-revealed transparent. |
| `bg-background` sur les inputs de form dans une page bg-background | Inputs invisibles. Utiliser `class="bg-card"`. |
| Centrer le texte dans les rows de nav sidebar (Settings) | `justify-center` hérité de Button - override avec `justify-start`. |
| Chips de filtre dont l'état actif n'est pas visuellement distinct | Pattern unique : raw button rounded-full avec `border-primary/40 bg-primary/10 text-primary`. |
| Compteurs (`count: 0`) qui restent à 0 jusqu'à click | Charger eager au mount, pas à l'ouverture du tab. |

---

## 11. Checklist pour une nouvelle page

Avant de coder une nouvelle route, vérifier :

1. **Famille de layout ?** A (sidebar+detail) / B (PageHeader+flat) / C (Settings-only) - réponse univoque.
2. **Si famille A** :
   - [ ] `mode === "split"` toujours sans PageHeader au-dessus
   - [ ] Auto-select du 1er item via `$effect`
   - [ ] Empty state pour le cas "0 entités" avec PageHeader+CTA full-width (famille B fallback)
   - [ ] Sidebar : header (titre · N) + search optional + filter chips + list + footer CTA
   - [ ] Right pane : header (logo/avatar + statut + nom + meta + actions) + TabBar underline + tab content
   - [ ] Tab content : `space-y-4 max-w-3xl` + cards `p-[14px_16px]`
3. **Sidebar rows** :
   - [ ] `<Button variant="ghost" size="auto">`
   - [ ] Accent bar `w-0.5`
   - [ ] Title `text-[12.5px]` + font-weight 600 quand actif
   - [ ] Subtitle `text-[10.5px] muted-foreground`
   - [ ] Active bg `bg-primary/10`
4. **Filter chips** :
   - [ ] Raw button `rounded-full border`
   - [ ] `.blur()` au click
   - [ ] Variantes compact (sidebar) ou full (list mode)
5. **Sheets** : uniquement wizards, catalogues, previews. **Jamais** le détail d'une entité éditable.
6. **Données externes** : charger eager au mount (counts, badges). Pas de "0 jusqu'à click".
7. **Forms** : `<FormField>` autour de chaque label+input. `<Input class="bg-card">` quand sur fond `bg-background`.
8. **Memory namespace** : si la page expose la mémoire d'un agent, utiliser `agent.memory_namespace` du manifest - **jamais** `agent.name`.

---

## 12. Référentiel par page (état mai 2026)

| Page | Famille | Statut |
|---|---|---|
| Dashboard | B | ✓ canon |
| Mon travail (Tasks list mode) | B | ✓ canon |
| Mon travail (Tasks split mode) | A | ✓ canon (refonte 2026-05-14) |
| Assistants | A | ✓ canon (refonte 2026-05-14) |
| Projets | A | ✓ canon (refonte 2026-05-15) |
| Connecteurs | A | ✓ canon (refonte 2026-05-15) |
| Mémoire | A-proche | ⚠️ similaire mais conserve son détail-Sheet (à aligner) |
| Activité (Tasks alias) | A | via mode split |
| Notifications | B | ✓ avec kicker |
| Inbox | B | ✓ avec tabs underline |
| Observabilité | B | ✓ avec tabs underline |
| Settings | C | ✓ left rail (justify-start) |
| Chat | spécial | tripartite (sidebar conversations / chat / config) - n'entre pas dans les 3 familles |
| Onboarding | spécial | flow modal multi-step |

---

## 13. Pour les futurs développements

**Si vous ajoutez une page** : commencer par identifier la famille. Si A, scaffolder à
partir de **Projets** (le canon le plus mature). Si B, à partir de **Dashboard**. Si C,
à partir de **Settings** (mais réfléchir à deux fois - Settings est l'unique cas C, ne pas
multiplier).

**Si vous touchez une primitive** (`$lib/components/operator/*` ou `$lib/components/ui/*`) :
mettre à jour ce document dans la même PR et flagger les consumers à reviewer.

**Si vous identifiez une nouvelle régression UX** : la documenter en section 10
(Anti-patterns) avec un commentaire `// REGRESSION 2026-XX-XX:` dans le code concerné.

---

*Dernière mise à jour : 2026-05-15 - fin de la refonte Connecteurs.*
