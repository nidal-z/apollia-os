<script lang="ts">
  /**
   * The tour's step card.
   *
   * Promoted from `operator/tours/` and pruned: the group-label props it carried
   * were never passed by anything after the original orchestrator was removed.
   *
   * Two presentations share this component. `modal` sits above a dimming
   * overlay; `annotated` sits beside a live interface and stays visually lighter
   * so it never reads as blocking the surface it points at. When the step has no
   * resolvable anchor, the card centres itself and says so rather than pointing
   * at nothing.
   *
   * Positioning is inline because it is computed at runtime from the anchor
   * geometry; everything else goes through utility classes and design tokens.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { ChevronLeft, ChevronRight, TriangleAlert, X } from "lucide-svelte";
  import { calculateCardPosition } from "./tour-utils";
  import type { TourPresentation } from "$lib/tour/types";

  interface Props {
    title: string;
    description: string;
    /** Zero-based index within the pre-flighted step list. */
    stepIndex: number;
    /** Number of steps retained after pre-flight, never the declared count. */
    totalSteps: number;
    /** Anchor geometry, `null` for the anchorless fallback. */
    targetRect: DOMRect | null;
    presentation: TourPresentation;
    onnext?: () => void;
    onprev?: () => void;
    onskip?: () => void;
  }

  let {
    title,
    description,
    stepIndex,
    totalSteps,
    targetRect,
    presentation,
    onnext,
    onprev,
    onskip,
  }: Props = $props();

  const CARD_WIDTH = 340;
  const CARD_HEIGHT_ESTIMATE = 190;
  const EDGE = 16;
  // Top offset for the anchorless `annotated` dock: clears the app header so the
  // card sits above the conversation column instead of over its centre.
  const TOP_DOCK = 80;

  let cardEl = $state<HTMLElement | null>(null);
  let cardHeight = $state(CARD_HEIGHT_ESTIMATE);
  let viewport = $state({ width: 0, height: 0 });

  let isLast = $derived(stepIndex === totalSteps - 1);
  let anchorless = $derived(targetRect === null);

  /**
   * Placed beside the anchor when there is one. Without an anchor: `annotated`
   * steps dock to the top edge, never the centre, so a step still waiting for
   * its live target (an approval card the model has not raised yet) stays
   * visible and navigable without covering the interface it annotates; `modal`
   * steps, which sit over a dimming overlay, centre as before.
   */
  let position = $derived(
    targetRect !== null
      ? calculateCardPosition(targetRect, viewport, { width: CARD_WIDTH, height: cardHeight })
      : presentation === "annotated"
        ? {
            top: TOP_DOCK,
            left: Math.max(EDGE, viewport.width / 2 - CARD_WIDTH / 2),
            placement: "bottom" as const,
          }
        : {
            top: Math.max(EDGE, viewport.height / 2 - cardHeight / 2),
            left: Math.max(EDGE, viewport.width / 2 - CARD_WIDTH / 2),
            placement: "bottom" as const,
          },
  );

  onMount(() => {
    viewport = { width: window.innerWidth, height: window.innerHeight };

    function onResize(): void {
      viewport = { width: window.innerWidth, height: window.innerHeight };
    }

    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  });

  $effect(() => {
    if (cardEl === null) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) cardHeight = entry.contentRect.height;
    });
    observer.observe(cardEl);
    return () => observer.disconnect();
  });
</script>

<div
  bind:this={cardEl}
  class="step-card glass-border flex flex-col rounded-xl bg-card px-4 pb-3 pt-3.5"
  class:shadow-elev-4={presentation === "modal"}
  class:shadow-elev-3={presentation === "annotated"}
  style:width="{CARD_WIDTH}px"
  style:top="{position.top}px"
  style:left="{position.left}px"
  data-placement={position.placement}
  data-testid="tour-step-card"
  role="dialog"
  aria-labelledby="tour-step-title"
  aria-describedby="tour-step-body"
>
  <div class="flex items-center justify-between gap-2">
    <span data-testid="tour-counter">
      <span class="sr-only">
        {$t("tour.chrome.counter_label", {
          values: { current: stepIndex + 1, total: totalSteps },
        })}
      </span>
      <span class="flex gap-0.5" aria-hidden="true">
        {#each { length: totalSteps } as _, i (i)}
          <span class="pip" class:on={i <= stepIndex}></span>
        {/each}
      </span>
    </span>
    <button
      type="button"
      class="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
      onclick={() => onskip?.()}
      aria-label={$t("tour.chrome.skip")}
      data-testid="tour-skip"
    >
      <X size={14} strokeWidth={2} />
    </button>
  </div>

  {#if anchorless}
    <span
      class="mt-1.5 inline-flex items-center gap-1.5 self-start rounded-full bg-warning/10 px-2 py-0.5 text-caption font-semibold text-warning"
      data-testid="tour-anchorless"
    >
      <TriangleAlert size={11} strokeWidth={2.2} />
      {$t("tour.chrome.anchorless")}
    </span>
  {/if}

  <h3 class="mb-1 mt-2 text-heading-sm text-foreground" id="tour-step-title">{title}</h3>
  <p class="text-body-xs text-muted-foreground" id="tour-step-body">{description}</p>

  <div class="mt-3 flex items-center gap-2 border-t border-border/60 pt-2.5">
    {#if stepIndex > 0}
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded-lg border border-border px-2.5 py-1.5 text-label-sm text-foreground transition-colors hover:bg-surface-2 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
        onclick={() => onprev?.()}
        data-testid="tour-prev"
      >
        <ChevronLeft size={16} strokeWidth={2} />
        {$t("tour.chrome.prev")}
      </button>
    {/if}

    <div class="flex-1"></div>

    <button
      type="button"
      class="inline-flex items-center gap-1 rounded-lg bg-primary-gradient px-2.5 py-1.5 text-label-sm text-primary-foreground shadow-primary-sm transition-all hover:-translate-y-px hover:shadow-primary-md focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
      onclick={() => onnext?.()}
      data-testid="tour-next"
    >
      {isLast ? $t("tour.chrome.finish") : $t("tour.chrome.next")}
      {#if !isLast}
        <ChevronRight size={16} strokeWidth={2} />
      {/if}
    </button>
  </div>
</div>

<style>
  /* Fixed placement is driven by the runtime anchor geometry, so width, top and
     left come from inline styles: the placement maths and the rendered box then
     share one source of truth and cannot drift. The rest lives here. */
  .step-card {
    position: fixed;
    z-index: calc(var(--z-overlay) + 2);
    max-width: calc(100vw - 2rem);
    transition:
      top var(--motion-slow) var(--ease-standard),
      left var(--motion-slow) var(--ease-standard);
  }

  .pip {
    width: 0.8125rem;
    height: 0.1875rem;
    border-radius: 0.125rem;
    background: hsl(var(--muted));
    transition: background var(--motion-base) var(--ease-standard);
  }

  .pip.on {
    background: linear-gradient(90deg, hsl(var(--grad-a)), hsl(var(--grad-b)));
  }

  @media (prefers-reduced-motion: reduce) {
    .step-card,
    .pip {
      transition: none;
    }
  }
</style>
