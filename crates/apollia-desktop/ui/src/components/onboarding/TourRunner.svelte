<script lang="ts">
  /**
   * `TourRunner` — mounts `TourSpotlight` + `TourStepCard` and drives steps
   * from a {@link ../../lib/tour/GuidedTour.ts} controller. Handles the
   * centred-card case when a step has a null `targetSelector`.
   *
   * Story:.
   */
  import { onMount, onDestroy } from 'svelte';
  import { t } from 'svelte-i18n';
  import TourSpotlight from './TourSpotlight.svelte';
  import TourStepCard from './TourStepCard.svelte';
  import { triggerTour, type TourController, type TourStep } from '$lib/tour/GuidedTour';

  interface Props {
    /** Registered tour id to run. */
    tourId: string;
    /** Dispatched when the tour finishes or is skipped. */
    oncomplete?: (reason: 'finish' | 'skip') => void;
  }

  let { tourId, oncomplete }: Props = $props();

  let controller = $state<TourController | null>(null);
  let currentIndex = $state(0);
  let targetRect = $state<DOMRect | null>(null);
  let initError = $state<string | null>(null);

  let unsubCurrent: (() => void) | null = null;
  let unsubEnd: (() => void) | null = null;
  let rectRaf: number | null = null;

  onMount(() => {
    try {
      controller = triggerTour(tourId);
    } catch (err: unknown) {
      initError = err instanceof Error ? err.message : String(err);
      return;
    }

    unsubCurrent = controller.current.subscribe((i) => {
      currentIndex = i;
    });
    unsubEnd = controller.onEnd((reason) => {
      oncomplete?.(reason);
    });
  });

  onDestroy(() => {
    unsubCurrent?.();
    unsubEnd?.();
    if (rectRaf !== null) cancelAnimationFrame(rectRaf);
  });

  let activeStep = $derived<TourStep | null>(
    controller !== null && controller.steps[currentIndex] !== undefined
      ? controller.steps[currentIndex]
      : null,
  );

  // Track the bounding rect of the active target; null selector → centred card.
  $effect(() => {
    if (rectRaf !== null) {
      cancelAnimationFrame(rectRaf);
      rectRaf = null;
    }

    const step = activeStep;
    if (step === null) return;

    if (step.targetSelector === null) {
      targetRect = null;
      return;
    }

    function poll(): void {
      const sel = step?.targetSelector;
      if (sel === null || sel === undefined) return;
      const el = document.querySelector(sel);
      targetRect = el !== null ? el.getBoundingClientRect() : null;
      rectRaf = requestAnimationFrame(poll);
    }
    poll();
  });

  function handleNext(): void {
    controller?.next();
  }
  function handlePrev(): void {
    controller?.prev();
  }
  function handleSkip(): void {
    controller?.skip();
  }
</script>

{#if initError !== null}
  <!-- Silent failure surface: log only, do not block UI. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div data-testid="tour-runner-error" hidden>{initError}</div>
{:else if controller !== null && activeStep !== null}
  {#if activeStep.targetSelector !== null}
    <TourSpotlight
      targetSelector={activeStep.targetSelector}
      onoverlaclick={handleSkip}
    />
  {/if}
  <TourStepCard
    title={$t(activeStep.titleKey)}
    description={$t(activeStep.bodyKey)}
    stepIndex={currentIndex}
    totalSteps={controller.steps.length}
    targetRect={targetRect}
    onnext={handleNext}
    onprev={handlePrev}
    onskip={handleSkip}
  />
{/if}
