<script lang="ts">
  /**
   * Dark-mode showcase.
   *
   * Dev-only page (not linked from the sidebar). Reached via the URL hash
   * `#design-dark-mode`. Exposes the key primitives - badges, surfaces,
   * inputs, glass tokens - so contrast/warmth regressions in dark mode
   * can be caught visually without spinning up each production page.
   */
  import { Separator } from "$lib/components/ui/separator";
  import { Badge } from "$lib/components/ui/badge";
  import { Input } from "$lib/components/ui/input";
  import { Check, AlertTriangle, Info, XCircle, Sparkles } from "lucide-svelte";

  const surfaces = [
    { name: "surface-1", cls: "bg-card text-card-foreground" },
    { name: "surface-2", cls: "bg-background text-foreground" },
    { name: "glass-card", cls: "glass-card" },
    { name: "glass-panel", cls: "glass-panel" },
    { name: "glass-surface", cls: "glass-surface" },
    { name: "glass-inset", cls: "glass-inset" },
  ];

  const badgeVariants = [
    "neutral",
    "primary",
    "success",
    "warning",
    "danger",
    "info",
    "outline",
  ] as const;
</script>

<div class="max-w-5xl space-y-10 py-6" data-testid="design-dark-mode-showcase">
  <header>
    <h1 class="text-display-lg text-foreground">Dark-mode primitives</h1>
    <p class="mt-2 text-sm text-muted-foreground md:text-base">
      Surfaces, badges, inputs and <code class="text-caption">--glass-*</code> tokens tested on
      dark warmth (<code class="text-caption">--surface-2</code>). Switch the theme with
      the header <code class="text-caption">ThemeToggle</code> to compare light and dark.
    </p>
  </header>

  <Separator />

  <!-- ── Surfaces + glass tokens ─────────────────────────────── -->
  <section class="space-y-3" data-testid="showcase-surfaces">
    <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">Surfaces</h2>
    <div class="grid grid-cols-2 gap-3 md:grid-cols-3">
      {#each surfaces as surface (surface.name)}
        <div
          class="{surface.cls} glass-border rounded-xl p-5"
          data-testid="surface-{surface.name}"
        >
          <p class="font-medium text-foreground">{surface.name}</p>
          <p class="mt-1 text-xs text-muted-a11y">Rim + warmth visibles</p>
        </div>
      {/each}
    </div>
  </section>

  <Separator />

  <!-- ── Badges ──────────────────────────────────────────────── -->
  <section class="space-y-3" data-testid="showcase-badges">
    <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">Badges</h2>
    <p class="text-xs text-muted-foreground">
      Variants <code class="text-caption">neutral | primary | success | warning | danger | info | outline</code>,
      sizes <code class="text-caption">sm | md</code>, optional icon.
    </p>
    <div class="space-y-3">
      {#each ["sm", "md"] as const as size (size)}
        <div class="flex flex-wrap items-center gap-2" data-testid="badges-size-{size}">
          <span class="text-xs text-muted-foreground w-10">{size}</span>
          {#each badgeVariants as variant (variant)}
            <Badge {variant} {size}>{variant}</Badge>
          {/each}
        </div>
      {/each}
      <div class="flex flex-wrap items-center gap-2" data-testid="badges-with-icon">
        <span class="text-xs text-muted-foreground w-10">icon</span>
        <Badge variant="success" size="sm">
          {#snippet icon()}<Check size={12} strokeWidth={2.5} />{/snippet}
          ok
        </Badge>
        <Badge variant="warning" size="sm">
          {#snippet icon()}<AlertTriangle size={12} strokeWidth={2.5} />{/snippet}
          warn
        </Badge>
        <Badge variant="danger" size="sm">
          {#snippet icon()}<XCircle size={12} strokeWidth={2.5} />{/snippet}
          fail
        </Badge>
        <Badge variant="info" size="md">
          {#snippet icon()}<Info size={14} strokeWidth={2.5} />{/snippet}
          info
        </Badge>
        <Badge variant="primary" size="md">
          {#snippet icon()}<Sparkles size={14} strokeWidth={2.5} />{/snippet}
          primary
        </Badge>
      </div>
    </div>
  </section>

  <Separator />

  <!-- ── Inputs + focus ring ─────────────────────────────────── -->
  <section class="space-y-3" data-testid="showcase-inputs">
    <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">Inputs</h2>
    <p class="text-xs text-muted-foreground">
      Focus ring driven by <code class="text-caption">--ring</code> (= primary). Tab into the fields
      to check the colour.
    </p>
    <div class="grid gap-3 md:grid-cols-2">
      <label class="flex flex-col gap-1">
        <span class="text-xs font-medium text-muted-a11y">Default</span>
        <Input placeholder="Placeholder text" aria-label="Default input showcase" />
      </label>
      <label class="flex flex-col gap-1">
        <span class="text-xs font-medium text-muted-a11y">Disabled</span>
        <Input placeholder="Read-only" disabled aria-label="Disabled input showcase" />
      </label>
    </div>
  </section>

  <Separator />

  <!-- ── Text contrast tokens ────────────────────────────────── -->
  <section class="space-y-3" data-testid="showcase-text">
    <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
      Text tokens (WCAG AA)
    </h2>
    <div class="glass-card glass-border rounded-xl p-5 space-y-1.5">
      <p class="text-foreground">Foreground - primary prose</p>
      <p class="text-muted-a11y">Muted - secondary prose</p>
      <p class="text-success-a11y">Success - verified ≥ 4.5:1</p>
      <p class="text-warning-a11y">Warning - verified ≥ 4.5:1</p>
      <p class="text-danger-a11y">Danger - verified ≥ 4.5:1</p>
    </div>
  </section>
</div>
