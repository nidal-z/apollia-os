<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { themeMode, type ThemeMode } from "$lib/stores/theme";
  import { elevation, primaryShadow, surface, glassBorder, backdrop } from "$lib/design/tokens";

  const elevations = [
    { key: "elev-0", token: elevation.elev0, label: "Flat / inline surfaces" },
    { key: "elev-1", token: elevation.elev1, label: "Cards, buttons at rest" },
    { key: "elev-2", token: elevation.elev2, label: "Raised cards, popovers" },
    { key: "elev-3", token: elevation.elev3, label: "Sheets, hovered cards" },
    { key: "elev-4", token: elevation.elev4, label: "Hero modals, spotlight" },
  ];

  const primaryShadows = [
    { key: "primary-sm", token: primaryShadow.sm },
    { key: "primary-md", token: primaryShadow.md },
    { key: "primary-lg", token: primaryShadow.lg },
    { key: "primary-xl", token: primaryShadow.xl },
  ];

  const surfaces = [
    { key: "--background", cssVar: "hsl(var(--background))" },
    { key: "--surface-1", cssVar: surface.s1 },
    { key: "--surface-2", cssVar: surface.s2 },
    { key: "--surface-3", cssVar: surface.s3 },
    { key: "--muted", cssVar: "hsl(var(--muted))" },
    { key: "--border", cssVar: "hsl(var(--border))" },
  ];

  const glassTokens = [
    { key: "--glass-border-light", value: glassBorder.light },
    { key: "--glass-border-light-hover", value: glassBorder.lightHover },
    { key: "--glass-border-dark", value: glassBorder.dark },
    { key: "--glass-border-dark-hover", value: glassBorder.darkHover },
  ];

  function toggleTheme(mode: ThemeMode) {
    themeMode.set(mode);
  }
</script>

<style>
  .design-swatch {
    box-shadow: var(--swatch-shadow);
  }
</style>

<div class="max-w-5xl space-y-10 py-6" data-testid="design-showcase">
  <header class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">Design tokens v2</h1>
      <p class="mt-1 text-sm text-muted-foreground">
        Elevation scale, warm dark palette, rim lights &amp; primary utilities
        &mdash; ADR-077.
      </p>
    </div>
    <div class="flex items-center gap-2">
      <Button
        variant={$themeMode === "light" ? "default" : "outline"}
        size="sm"
        onclick={() => toggleTheme("light")}
      >
        Light
      </Button>
      <Button
        variant={$themeMode === "dark" ? "default" : "outline"}
        size="sm"
        onclick={() => toggleTheme("dark")}
      >
        Dark
      </Button>
    </div>
  </header>

  <!-- Elevation scale -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Elevation scale
    </h2>
    <div class="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-5">
      {#each elevations as { key, token, label } (key)}
        <div class="space-y-2">
          <div
            class="design-swatch flex h-28 items-center justify-center rounded-xl bg-surface-1 text-sm font-medium text-foreground"
            style="--swatch-shadow: {token};"
            data-elev={key}
          >
            {key}
          </div>
          <p class="text-xs text-muted-foreground">{label}</p>
        </div>
      {/each}
    </div>
  </section>

  <!-- Primary shadows -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Primary-tinted shadows
    </h2>
    <div class="grid grid-cols-2 gap-5 sm:grid-cols-4">
      {#each primaryShadows as { key, token } (key)}
        <div class="space-y-2">
          <div
            class="design-swatch flex h-24 items-center justify-center rounded-xl bg-primary-gradient text-sm font-medium text-primary-foreground"
            style="--swatch-shadow: {token};"
          >
            {key}
          </div>
        </div>
      {/each}
    </div>
  </section>

  <!-- Primary utilities -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Primary utilities
    </h2>
    <div class="flex flex-wrap gap-3">
      <Button variant="ghost" size="sm" class="bg-primary-solid inline-flex h-10 items-center rounded-md px-4 text-sm font-medium">
        bg-primary-solid
      </Button>
      <Button variant="ghost" size="sm" class="bg-primary-gradient inline-flex h-10 items-center rounded-md px-4 text-sm font-medium">
        bg-primary-gradient
      </Button>
      <span class="border-primary-subtle inline-flex h-10 items-center rounded-md bg-background px-4 text-sm font-medium text-foreground">
        border-primary-subtle
      </span>
    </div>
  </section>

  <!-- Surface / palette -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Surface palette &mdash; {$themeMode ?? "light"}
    </h2>
    <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
      {#each surfaces as { key, cssVar } (key)}
        <div
          class="flex h-24 flex-col items-start justify-end rounded-lg p-3 shadow-elev-1"
          style="background: {cssVar};"
        >
          <span class="rounded bg-background/60 px-1.5 py-0.5 text-[10px] font-mono text-foreground backdrop-blur-sm">
            {key}
          </span>
        </div>
      {/each}
    </div>
  </section>

  <!-- Glass tokens -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Glass borders &amp; inset
    </h2>
    <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
      {#each glassTokens as { key, value } (key)}
        <div class="rounded-xl border p-4 text-xs font-mono" style="border-color: {value};">
          {key}
        </div>
      {/each}
    </div>
    <div class="grid grid-cols-2 gap-3 sm:grid-cols-3">
      <div class="glass-inset rounded-lg p-4 text-xs text-foreground">.glass-inset</div>
      <div class="glass-surface rounded-lg p-4 text-xs text-foreground">.glass-surface</div>
      <div class="glass-card rounded-lg p-4 text-xs text-foreground">.glass-card</div>
    </div>
  </section>

  <!-- Backdrop -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Backdrop
    </h2>
    <div class="relative h-40 overflow-hidden rounded-xl bg-surface-1 p-4 shadow-elev-2">
      <p class="text-sm text-foreground">Content behind backdrop.</p>
      <div class="app-backdrop absolute inset-0 flex items-center justify-center">
        <span class="rounded-md bg-background/80 px-3 py-1.5 text-xs font-medium text-foreground">
          .app-backdrop (backdrop: {backdrop.default})
        </span>
      </div>
    </div>
  </section>
</div>
