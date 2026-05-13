<script lang="ts">
  /**
   * Motion showcase.
   *
   * Dev-only page (not linked from the sidebar). Reached via the URL hash
   * `#motion`. Demonstrates every motion primitive shipped by the sprint:
   * gradients, list-item spring, dividers, loading states, scrollbar.
   */
  import { Button } from "$lib/components/ui/button";
  import { Separator } from "$lib/components/ui/separator";
  import { themeMode, type ThemeMode } from "$lib/stores/theme";
  import { LoadingShimmer, LoadingSpinner } from "$lib/components/feedback";
  import { PageTransition } from "$lib/components/motion";
  import { duration, spring, prefersReducedMotion } from "$lib/design/motion";

  const gradients = [
    { key: "--gradient-primary", className: "bg-gradient-primary text-white", label: "Hero CTAs, promoted headers" },
    { key: "--gradient-surface", className: "bg-gradient-surface text-foreground", label: "Page hero backgrounds" },
    { key: "--gradient-accent", className: "bg-gradient-accent text-foreground", label: "Empty states, soft washes" },
  ];

  const listItems = [
    { label: "Home", hint: "hover me — translateX + scale" },
    { label: "Assistants", hint: "press & hold for active state" },
    { label: "Projects" },
    { label: "Activity" },
    { label: "Chat" },
  ];

  function toggleTheme(mode: ThemeMode) {
    themeMode.set(mode);
  }

  let refreshKey = $state(0);
  const reduced = prefersReducedMotion();
</script>

<div class="max-w-5xl space-y-10 py-6" data-testid="design-motion-showcase">
  <header class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">Motion &amp; gradients</h1>
      <p class="mt-1 text-sm text-muted-foreground">
        Spring presets, canonical gradients, loading states.
      </p>
      {#if reduced}
        <p class="mt-2 text-xs font-medium text-warning-foreground">
          <span class="inline-block h-1.5 w-1.5 rounded-full bg-warning align-middle"></span>
          Reduced motion is active &mdash; animations are collapsed.
        </p>
      {/if}
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

  <!-- Gradients -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Canonical gradients
    </h2>
    <div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
      {#each gradients as { key, className, label } (key)}
        <div
          class="flex h-28 flex-col justify-end rounded-xl px-4 py-3 shadow-elev-1 {className}"
          data-gradient={key}
        >
          <span class="text-xs font-mono opacity-90">{key}</span>
          <span class="text-[11px] opacity-80">{label}</span>
        </div>
      {/each}
    </div>
  </section>

  <!-- Spring list items -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      List-item spring &mdash; <code class="text-[10px]">.list-item-spring</code>
    </h2>
    <p class="text-xs text-muted-foreground/70">
      stiffness {spring.default.stiffness} · damping {spring.default.damping}
    </p>
    <div class="max-w-sm space-y-1 rounded-xl glass-card glass-border p-2">
      {#each listItems as item (item.label)}
        <Button variant="ghost" size="sm"
          class="list-item-spring flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm text-foreground hover:bg-primary/[0.06]"
        >
          <span class="h-2 w-2 rounded-full bg-primary/60"></span>
          <span class="flex-1">{item.label}</span>
          {#if item.hint}<span class="text-[10px] text-muted-foreground/60">{item.hint}</span>{/if}
        </Button>
      {/each}
    </div>
  </section>

  <!-- Page transition -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Page transition &mdash; <code class="text-[10px]">&lt;PageTransition&gt;</code>
    </h2>
    <div class="flex items-center gap-3">
      <Button size="sm" variant="outline" onclick={() => refreshKey++}>
        Trigger entrance
      </Button>
      <span class="text-xs text-muted-foreground/70">
        fly y=8 · duration {duration.base}ms
      </span>
    </div>
    <div class="rounded-xl border border-border/60 bg-surface-2 p-6">
      {#key refreshKey}
        <PageTransition>
          <div class="rounded-lg bg-gradient-accent p-6 text-center text-foreground">
            <span class="text-sm font-medium">Content slides up + fades in.</span>
          </div>
        </PageTransition>
      {/key}
    </div>
  </section>

  <!-- Dividers -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Dividers
    </h2>
    <div class="rounded-xl glass-card glass-border p-5 space-y-4">
      <span class="text-xs text-muted-foreground">Solid</span>
      <Separator />
      <span class="text-xs text-muted-foreground">Fade edges (.divider-fade)</span>
      <Separator variant="fade" />
    </div>
  </section>

  <!-- Loading states -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Loading states
    </h2>
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
      <div class="space-y-2 rounded-xl glass-card glass-border p-4">
        <span class="text-xs text-muted-foreground">LoadingShimmer</span>
        <LoadingShimmer width="100%" height="2rem" />
        <LoadingShimmer width="80%" height="2rem" />
        <LoadingShimmer width="60%" height="2rem" />
      </div>
      <div class="flex flex-col items-center justify-center gap-3 rounded-xl glass-card glass-border p-4">
        <span class="text-xs text-muted-foreground">LoadingSpinner</span>
        <div class="flex items-center gap-5">
          <LoadingSpinner size={14} tone="muted" />
          <LoadingSpinner size={20} tone="primary" />
          <LoadingSpinner size={28} tone="primary" />
        </div>
      </div>
    </div>
  </section>

  <!-- Scrollbar -->
  <section class="space-y-3">
    <h2 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">
      Scrollbar
    </h2>
    <div class="max-h-40 overflow-auto rounded-xl glass-card glass-border p-4">
      <div class="space-y-2">
        {#each Array.from({ length: 20 }, (_, i) => i) as i}
          <p class="text-xs text-muted-foreground">Scrollable row #{i + 1} — 6px rail, translucent thumb.</p>
        {/each}
      </div>
    </div>
  </section>
</div>
