<script lang="ts">
  /**
   * EmptyState - canonical empty-state surface.
   *
   * Narrative, engaging empty state: gradient illustration, display-sm title,
   * body description, primary + optional secondary CTA, optional trailing slot
   * for contextual stats. Honours `prefers-reduced-motion`.
   */
  import type { Snippet } from "svelte";
  import type { Icon } from "lucide-svelte";
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { Button } from "$lib/components/ui/button";
  import { duration, prefersReducedMotion } from "$lib/design/motion";

  interface Props {
    /** Lucide icon - renders inside the default gradient illustration. Ignored when `illustration` is passed. */
    icon?: typeof Icon;
    /** Headline - rendered at `text-display-sm`. */
    title: string;
    /** 1–2 sentences explaining the state and what the user can do next. */
    description?: string;
    /** Primary CTA label - paired with `onPrimary`. */
    primaryLabel?: string;
    primaryAction?: () => void;
    /** Secondary CTA label - paired with `onSecondary`. Rendered as ghost button. */
    secondaryLabel?: string;
    secondaryAction?: () => void;
    /** Used for `data-testid="empty-state-${page}"` - lets E2E tests target this instance. */
    page?: string;
    /** Override the default icon-in-gradient illustration. */
    illustration?: Snippet;
    /** Right-aligned contextual stat / pill (e.g. "3 connected" or "5 last week"). */
    trailing?: Snippet;
  }

  let {
    icon: IconComponent,
    title,
    description,
    primaryLabel,
    primaryAction,
    secondaryLabel,
    secondaryAction,
    page,
    illustration,
    trailing,
  }: Props = $props();

  const reduced = prefersReducedMotion();
  const flyY = $derived(reduced ? 0 : 12);
  const flyDuration = $derived(reduced ? 0 : duration.slow);
</script>

<div
  class="relative flex flex-col items-center justify-center gap-5 overflow-hidden rounded-2xl px-6 py-16 text-center glass-card glass-border shadow-elev-1"
  data-testid={page ? `empty-state-${page}` : "empty-state"}
  in:fly={{ y: flyY, duration: flyDuration, easing: cubicOut }}
>
  <!-- Accent wash - tints the background with the brand gradient. -->
  <div class="pointer-events-none absolute inset-0 bg-gradient-accent opacity-70" aria-hidden="true"></div>

  {#if trailing}
    <div class="absolute right-4 top-4 z-10">{@render trailing()}</div>
  {/if}

  <!-- Illustration : custom snippet OR default gradient + icon. -->
  <div class="relative">
    {#if illustration}
      {@render illustration()}
    {:else if IconComponent}
      <div
        class="flex h-20 w-20 items-center justify-center rounded-2xl bg-gradient-primary shadow-primary-lg"
        aria-hidden="true"
      >
        <IconComponent size={36} strokeWidth={1.5} class="text-primary-foreground/95" />
      </div>
    {/if}
  </div>

  <div class="relative flex max-w-xl flex-col items-center gap-2">
    <h2 class="text-display-sm text-foreground" data-testid="empty-state-title">
      {title}
    </h2>
    {#if description}
      <p
        class="text-sm leading-relaxed text-muted-foreground md:text-base"
        data-testid="empty-state-description"
      >
        {description}
      </p>
    {/if}
  </div>

  {#if (primaryLabel && primaryAction) || (secondaryLabel && secondaryAction)}
    <div class="relative mt-1 flex flex-col items-stretch gap-2 sm:flex-row sm:items-center">
      {#if primaryLabel && primaryAction}
        <Button
          variant="primary-gradient"
          onclick={primaryAction}
          data-testid="empty-state-primary-cta"
          class="min-w-[10rem]"
        >
          {primaryLabel}
        </Button>
      {/if}
      {#if secondaryLabel && secondaryAction}
        <Button
          variant="ghost"
          onclick={secondaryAction}
          data-testid="empty-state-secondary-cta"
        >
          {secondaryLabel}
        </Button>
      {/if}
    </div>
  {/if}
</div>
