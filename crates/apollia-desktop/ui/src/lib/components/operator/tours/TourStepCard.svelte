<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from 'svelte-i18n';
  import { ChevronLeft, ChevronRight, X } from 'lucide-svelte';
  import { calculateCardPosition } from './tour-utils';

  interface Props {
    /** Step title. */
    title: string;
    /** Step description (plain text). */
    description: string;
    /** Zero-based index of the current step. */
    stepIndex: number;
    /** Total number of steps in the tour. */
    totalSteps: number;
    /** Bounding rect of the highlighted element, used to compute card placement. */
    targetRect: DOMRect | null;
    /** Group label for the counter (e.g. "Dashboard"). */
    groupLabel?: string;
    /** Zero-based index within the current group. */
    subStepIndex?: number;
    /** Total sub-steps in the current group. */
    subStepCount?: number;
    /** Show the "Next" button. */
    showNext?: boolean;
    /** Show the "Previous" button. */
    showPrev?: boolean;
    /** Show the "Skip" button. */
    showSkip?: boolean;
    /** Called when the user clicks "Next". */
    onnext?: () => void;
    /** Called when the user clicks "Previous". */
    onprev?: () => void;
    /** Called when the user clicks "Skip". */
    onskip?: () => void;
  }

  let {
    title,
    description,
    stepIndex,
    totalSteps,
    targetRect,
    groupLabel = '',
    subStepIndex = 0,
    subStepCount = 1,
    showNext = true,
    showPrev = true,
    showSkip = true,
    onnext,
    onprev,
    onskip,
  }: Props = $props();

  let counterText = $derived(
    groupLabel && subStepCount > 1
      ? `${groupLabel} (${subStepIndex + 1}/${subStepCount})`
      : groupLabel || `${stepIndex + 1} / ${totalSteps}`
  );

  const CARD_WIDTH = 360;
  const CARD_HEIGHT_ESTIMATE = 200;
  const FALLBACK_POSITION = { top: 16, left: 16, placement: 'bottom' as const };

  let cardEl = $state<HTMLElement | null>(null);
  let cardHeight = $state(CARD_HEIGHT_ESTIMATE);
  let viewport = $state({ width: 0, height: 0 });

  let position = $derived(
    targetRect !== null
      ? calculateCardPosition(
          targetRect,
          viewport,
          { width: CARD_WIDTH, height: cardHeight },
        )
      : FALLBACK_POSITION,
  );

  onMount(() => {
    viewport = {
      width: window.innerWidth,
      height: window.innerHeight,
    };

    function onResize(): void {
      viewport = { width: window.innerWidth, height: window.innerHeight };
    }

    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });

  // Measure the card's actual rendered height for accurate placement.
  $effect(() => {
    if (cardEl === null) return;

    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        cardHeight = entry.contentRect.height;
      }
    });

    ro.observe(cardEl);
    return () => ro.disconnect();
  });
</script>

<div
  bind:this={cardEl}
  class="step-card"
  style:top="{position.top}px"
  style:left="{position.left}px"
  data-placement={position.placement}
  data-testid="tour-step-card"
>
  <div class="card-header">
    <span class="step-counter">{counterText}</span>
    {#if showSkip}
      <button
        type="button"
        class="skip-btn"
        onclick={() => onskip?.()}
        aria-label={$t("onboarding_v2.tour.skip_label")}
        data-testid="tour-skip"
      >
        <X size={14} strokeWidth={2} />
      </button>
    {/if}
  </div>

  <h3 class="card-title">{title}</h3>
  <p class="card-description">{description}</p>

  <div class="card-actions">
    {#if showPrev}
      <button
        type="button"
        class="nav-btn nav-btn-prev"
        onclick={() => onprev?.()}
        disabled={stepIndex === 0}
        data-testid="tour-prev"
      >
        <ChevronLeft size={16} strokeWidth={2} />
        {$t("onboarding_v2.tour.prev")}
      </button>
    {/if}

    <div class="actions-spacer"></div>

    {#if showNext}
      <button
        type="button"
        class="nav-btn nav-btn-next"
        onclick={() => onnext?.()}
        data-testid="tour-next"
      >
        {stepIndex === totalSteps - 1 ? $t("onboarding_v2.tour.finish") : $t("onboarding_v2.tour.next")}
        {#if stepIndex < totalSteps - 1}
          <ChevronRight size={16} strokeWidth={2} />
        {/if}
      </button>
    {/if}
  </div>
</div>

<style>
  .step-card {
    position: fixed;
    z-index: 62;
    width: 360px;
    max-width: calc(100vw - 32px);
    background: hsl(var(--card));
    border-radius: 12px;
    padding: 1.25rem;
    box-shadow: var(--shadow-elev-4);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    transition:
      top 400ms var(--ease-standard),
      left 400ms var(--ease-standard);
  }

  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .step-counter {
    font-size: 0.75rem;
    font-weight: 600;
    color: hsl(var(--muted-foreground) / 0.7);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .skip-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    border: none;
    background: none;
    border-radius: 0.375rem;
    color: hsl(var(--muted-foreground) / 0.7);
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease;
    padding: 0;
  }

  .skip-btn:hover {
    background: hsl(var(--foreground) / 0.06);
    color: hsl(var(--muted-foreground));
  }

  .card-title {
    font-size: 1rem;
    font-weight: 700;
    color: hsl(var(--foreground));
    margin: 0;
    line-height: 1.3;
    letter-spacing: -0.01em;
  }

  .card-description {
    font-size: 0.875rem;
    color: hsl(var(--foreground) / 0.8);
    margin: 0;
    line-height: 1.55;
  }

  .card-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }

  .actions-spacer {
    flex: 1;
  }

  .nav-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.5rem 0.875rem;
    border-radius: 0.5rem;
    border: none;
    font-size: 0.8125rem;
    font-weight: 600;
    cursor: pointer;
    transition: transform 120ms ease, box-shadow 120ms ease, background 120ms ease;
    line-height: 1;
  }

  .nav-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .nav-btn-prev {
    background: transparent;
    color: hsl(var(--muted-foreground));
    border: 1px solid hsl(var(--foreground) / 0.1);
  }

  .nav-btn-prev:hover:not(:disabled) {
    background: hsl(var(--foreground) / 0.04);
  }

  .nav-btn-next {
    background: linear-gradient(135deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)));
    color: hsl(var(--primary-foreground));
    box-shadow: var(--shadow-primary-sm);
  }

  .nav-btn-next:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: var(--shadow-primary-md);
  }

  .nav-btn-next:active:not(:disabled) {
    transform: translateY(0);
  }
</style>
