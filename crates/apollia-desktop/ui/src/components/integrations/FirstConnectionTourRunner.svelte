<script lang="ts">
  /**
   * Lightweight 4-step spotlight tour launched after the first successful
   * connection (C.I.14). Reuses `TourSpotlight` + `TourStepCard`
   * primitives from the onboarding framework but does NOT touch the onboarding
   * phase machine. Skippable via Esc or the Skip button.
   */
  import { onMount, tick } from "svelte";
  import { t } from "svelte-i18n";
  import TourSpotlight from "$lib/components/operator/tours/TourSpotlight.svelte";
  import TourStepCard from "$lib/components/operator/tours/TourStepCard.svelte";
  import {
    FIRST_CONNECTION_STEPS,
    markFirstConnectionTourDone,
  } from "$lib/tour/FirstConnectionTour";

  interface Props {
    onclose: () => void;
  }

  let { onclose }: Props = $props();

  let stepIndex = $state(0);
  let targetRect = $state<DOMRect | null>(null);

  const currentStep = $derived(FIRST_CONNECTION_STEPS[stepIndex] ?? null);
  const totalSteps = FIRST_CONNECTION_STEPS.length;

  async function resolveRect(): Promise<void> {
    await tick();
    if (currentStep === null) return;
    const sel = currentStep.spotlightSelector;
    if (sel === null) {
      targetRect = null;
      return;
    }
    const el = document.querySelector(sel);
    targetRect = el !== null ? el.getBoundingClientRect() : null;
  }

  onMount(() => {
    void resolveRect();
    const keyHandler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        finish();
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        void next();
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        void prev();
      }
    };
    window.addEventListener("keydown", keyHandler);
    const resizeHandler = () => void resolveRect();
    window.addEventListener("resize", resizeHandler);
    return () => {
      window.removeEventListener("keydown", keyHandler);
      window.removeEventListener("resize", resizeHandler);
    };
  });

  async function next(): Promise<void> {
    if (stepIndex >= totalSteps - 1) {
      finish();
      return;
    }
    stepIndex += 1;
    await resolveRect();
  }

  async function prev(): Promise<void> {
    if (stepIndex === 0) return;
    stepIndex -= 1;
    await resolveRect();
  }

  function finish(): void {
    markFirstConnectionTourDone();
    onclose();
  }
</script>

{#if currentStep !== null}
  <TourSpotlight
    targetSelector={currentStep.spotlightSelector ?? ""}
    visible={currentStep.spotlightSelector !== null}
    onoverlaclick={finish}
  />

  <TourStepCard
    title={$t(currentStep.titleKey)}
    description={$t(currentStep.bodyKey)}
    stepIndex={stepIndex}
    {totalSteps}
    {targetRect}
    groupLabel=""
    subStepIndex={stepIndex}
    subStepCount={totalSteps}
    showPrev={stepIndex > 0}
    showNext={true}
    showSkip={true}
    onnext={() => void next()}
    onprev={() => void prev()}
    onskip={finish}
  />
{/if}
