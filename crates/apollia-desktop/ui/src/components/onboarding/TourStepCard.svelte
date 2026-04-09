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
    background: #ffffff;
    border-radius: 12px;
    padding: 1.25rem;
    box-shadow:
      0 0 0 1px rgba(52, 53, 245, 0.08),
      0 4px 16px -2px rgba(0, 0, 0, 0.1),
      0 16px 48px -8px rgba(52, 53, 245, 0.12);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    transition:
      top 400ms cubic-bezier(0.4, 0, 0.2, 1),
      left 400ms cubic-bezier(0.4, 0, 0.2, 1);
  }

  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .step-counter {
    font-size: 0.75rem;
    font-weight: 600;
    color: #9ca3af;
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
    color: #9ca3af;
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease;
    padding: 0;
  }

  .skip-btn:hover {
    background: rgba(0, 0, 0, 0.06);
    color: #6b7280;
  }

  .card-title {
    font-size: 1rem;
    font-weight: 700;
    color: #1a1a2e;
    margin: 0;
    line-height: 1.3;
    letter-spacing: -0.01em;
  }

  .card-description {
    font-size: 0.875rem;
    color: #4b5563;
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
    color: #6b7280;
    border: 1px solid rgba(0, 0, 0, 0.1);
  }

  .nav-btn-prev:hover:not(:disabled) {
    background: rgba(0, 0, 0, 0.04);
  }

  .nav-btn-next {
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    color: #ffffff;
    box-shadow: 0 2px 8px -1px rgba(52, 53, 245, 0.35);
  }

  .nav-btn-next:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 4px 14px -2px rgba(52, 53, 245, 0.45);
  }

  .nav-btn-next:active:not(:disabled) {
    transform: translateY(0);
  }
</style>
