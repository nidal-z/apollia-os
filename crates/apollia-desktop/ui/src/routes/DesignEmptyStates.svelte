<script lang="ts">
  /**
   * Empty-state showcase.
   *
   * Dev-only page (not linked from the sidebar). Reached via the URL hash
   * `#design-empty-states`. Renders the 7 canonical empty-state variants from
   * the i18n catalogue so designers and engineers can eyeball copy, spacing,
   * and illustration consistency without having to empty each production page.
   */
  import { t } from "svelte-i18n";
  import { Separator } from "$lib/components/ui/separator";
  import { EmptyState } from "$lib/components/layout";
  import {
    EMPTY_STATES,
    EMPTY_STATE_ORDER,
    type EmptyStateVariant,
  } from "$lib/i18n/strings/empty-states";
  import { prefersReducedMotion } from "$lib/design/motion";

  const reduced = prefersReducedMotion();

  /** Stub actions - the showcase doesn't wire real handlers. */
  function noop(): void {
    /* showcase action */
  }

  function variantTestId(variant: EmptyStateVariant): string {
    return `empty-state-showcase-${variant}`;
  }
</script>

<div class="max-w-5xl space-y-10 py-6" data-testid="design-empty-states-showcase">
  <header>
    <h1 class="text-display-lg text-foreground">Empty states</h1>
    <p class="mt-2 text-sm text-muted-foreground md:text-base">
      Canonical <code class="text-caption">&lt;EmptyState&gt;</code> component - 7 variants sourced from
      <code class="text-caption">$lib/i18n/strings/empty-states</code>. Copies tonales, primary + secondary CTA, slot
      <code class="text-caption">trailing</code> for a contextual stat.
    </p>
    {#if reduced}
      <p class="mt-3 text-xs font-medium text-warning-foreground">
        <span class="mr-1 inline-block h-1.5 w-1.5 rounded-full bg-warning align-middle"></span>
        Reduced motion is active - entrance animations are collapsed.
      </p>
    {/if}
  </header>

  <Separator />

  <div class="grid grid-cols-1 gap-6">
    {#each EMPTY_STATE_ORDER as variant (variant)}
      {@const entry = EMPTY_STATES[variant]}
      <section class="space-y-3" data-testid={variantTestId(variant)}>
        <div class="flex items-baseline justify-between">
          <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            {variant}
          </h2>
          <code class="text-micro text-muted-foreground/60">
            {entry.titleKey}
          </code>
        </div>

        <EmptyState
          icon={entry.icon}
          title={$t(entry.titleKey)}
          description={$t(entry.descriptionKey)}
          primaryLabel={entry.primaryCtaKey ? $t(entry.primaryCtaKey) : undefined}
          primaryAction={entry.primaryCtaKey ? noop : undefined}
          secondaryLabel={entry.secondaryCtaKey ? $t(entry.secondaryCtaKey) : undefined}
          secondaryAction={entry.secondaryCtaKey ? noop : undefined}
          page={entry.page}
        >
          {#snippet trailing()}
            <span
              class="inline-flex items-center rounded-full leading-none border border-border/40 bg-background/60 px-2 py-0.5 text-micro font-medium text-muted-foreground backdrop-blur-sm"
            >
              {variant}
            </span>
          {/snippet}
        </EmptyState>
      </section>
    {/each}
  </div>
</div>
