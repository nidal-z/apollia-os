<script lang="ts">
  /**
   * Minimal post-onboarding guided tour runner.
   *
   * Fetches the step list via `get_tour_steps(profile)`, then cycles through
   * each step using the existing `TourSpotlight` + `TourStepCard` primitives.
   * On Next, persists the step's completion via `complete_tour_step`. On
   * Skip / final Next, fires `oncomplete` so the App can clean up.
   *
   * Note (May 2026): the underlying tour content is partially obsolete and
   * scheduled for a rework — we wire the orchestration here so the
   * post-onboarding hand-off is no longer a hard cut to the dashboard, but
   * the steps themselves are best-effort.
   */
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import TourSpotlight from "./TourSpotlight.svelte";
  import TourStepCard from "./TourStepCard.svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import type { OnboardingState, TourStep } from "$lib/types";

  interface Props {
    /** Fired when the user finishes (or skips) the tour. */
    oncomplete: () => void;
  }

  const { oncomplete }: Props = $props();

  let steps = $state<TourStep[]>([]);
  let currentIndex = $state(0);
  let targetRect = $state<DOMRect | null>(null);
  let profile = $state<string>("operator");
  let loading = $state(true);

  const currentStep = $derived(steps[currentIndex] ?? null);

  async function loadSteps(): Promise<void> {
    try {
      const state = await invoke<OnboardingState>("get_onboarding_state");
      profile = state.profile ?? "operator";
      const result = await invoke<TourStep[]>("get_tour_steps", { profile });
      steps = result;
    } catch (err) {
      console.warn("[OnboardingTourRunner] failed to load steps:", err);
      steps = [];
    } finally {
      loading = false;
      // No steps to show → fire complete immediately so the App layer can
      // continue to the dashboard.
      if (steps.length === 0) oncomplete();
      else void activate(0);
    }
  }

  async function activate(index: number): Promise<void> {
    const step = steps[index];
    if (step === undefined) return;

    if (step.route) {
      try {
        navigateTo(step.route as never);
      } catch {
        /* unknown route — leave current view */
      }
    }
    // Wait for the route's components to mount before measuring the target.
    await tick();
    await new Promise((r) => setTimeout(r, 120));

    if (step.spotlight_selector) {
      const el = document.querySelector(step.spotlight_selector);
      targetRect = el?.getBoundingClientRect() ?? null;
    } else {
      targetRect = null;
    }
  }

  async function next(): Promise<void> {
    const step = steps[currentIndex];
    if (step !== undefined) {
      try {
        await invoke("complete_tour_step", { stepId: step.id, profile });
      } catch (err) {
        console.warn("[OnboardingTourRunner] complete_tour_step failed:", err);
      }
    }
    if (currentIndex >= steps.length - 1) {
      oncomplete();
      return;
    }
    currentIndex += 1;
    await activate(currentIndex);
  }

  async function prev(): Promise<void> {
    if (currentIndex <= 0) return;
    currentIndex -= 1;
    await activate(currentIndex);
  }

  function skip(): void {
    oncomplete();
  }

  onMount(() => {
    void loadSteps();
  });
</script>

{#if !loading && currentStep !== null}
  <TourSpotlight
    targetSelector={currentStep.spotlight_selector ?? ""}
    onoverlaclick={skip}
  />
  <TourStepCard
    title={$t(`${currentStep.companion_message_key}.title`, {
      default: currentStep.id,
    } as Record<string, unknown>)}
    description={$t(`${currentStep.companion_message_key}.description`, {
      default: "",
    } as Record<string, unknown>)}
    stepIndex={currentIndex}
    totalSteps={steps.length}
    {targetRect}
    showPrev={currentIndex > 0}
    showSkip={true}
    onnext={next}
    onprev={prev}
    onskip={skip}
  />
{/if}
