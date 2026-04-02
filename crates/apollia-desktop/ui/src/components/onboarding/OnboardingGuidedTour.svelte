<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { get } from "svelte/store";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { navigateTo } from "$lib/stores/navigation";
  import TourSpotlight from "./TourSpotlight.svelte";
  import TourStepCard from "./TourStepCard.svelte";
  import TourProgressRail from "./TourProgressRail.svelte";
  import type { TourStep, OnboardingPhase } from "$lib/types";
  import type { Route } from "$lib/stores/navigation";

  // ─── State ─────────────────────────────────────────────────────────────────

  let steps = $state<TourStep[]>([]);
  let stepIndex = $state(0);
  let targetRect = $state<DOMRect | null>(null);
  let loading = $state(true);
  let skipping = $state(false);
  let showConfirmExit = $state(false);
  let autoTimer = $state<ReturnType<typeof setTimeout> | null>(null);
  let waitEventUnlisten = $state<UnlistenFn | null>(null);

  // ─── Derived helpers ────────────────────────────────────────────────────────

  let currentStep = $derived(steps[stepIndex] ?? null);
  let totalSteps = $derived(steps.length);

  // ─── Lifecycle ─────────────────────────────────────────────────────────────

  onMount(() => {
    void loadAndStart();

    return () => {
      clearAutoTimer();
      clearWaitEvent();
    };
  });

  // ─── Load & start ──────────────────────────────────────────────────────────

  async function loadAndStart(): Promise<void> {
    const state = get(onboardingStore);
    const profile = state.profile ?? "operator";

    let loaded: TourStep[];
    try {
      loaded = await invoke<TourStep[]>("get_tour_steps", { profile });
    } catch (err) {
      console.error("[GuidedTour] get_tour_steps failed:", err);
      loading = false;
      return;
    }

    steps = loaded;

    // Restore persisted progress, clamped to valid range.
    const persisted = state.tour_step_index ?? 0;
    stepIndex = Math.min(Math.max(0, persisted), Math.max(0, loaded.length - 1));

    loading = false;

    await activateStep(stepIndex);
  }

  // ─── Step activation ───────────────────────────────────────────────────────

  async function activateStep(index: number): Promise<void> {
    clearAutoTimer();
    clearWaitEvent();

    const step = steps[index];
    if (step === undefined) return;

    // Navigate to the step's route before DOM operations.
    const route = step.route.replace(/^\//, "") as Route;
    navigateTo(route);

    // Wait for Svelte to flush and the browser to paint.
    await tick();
    await sleep(80);

    // Resolve the spotlight target with retry logic.
    targetRect = await resolveSelector(step.spotlight_selector);

    // Set up completion based on the mode.
    scheduleCompletion(step);
  }

  // ─── Selector resolution (3 retries × 500 ms) ─────────────────────────────

  async function resolveSelector(selector: string | null): Promise<DOMRect | null> {
    if (selector === null) return null;

    for (let attempt = 0; attempt < 3; attempt++) {
      const el = document.querySelector(selector);
      if (el !== null) {
        return el.getBoundingClientRect();
      }
      if (attempt < 2) {
        await sleep(500);
      }
    }

    console.warn(`[GuidedTour] selector not found after 3 attempts: "${selector}" — skipping spotlight`);
    return null;
  }

  // ─── Completion scheduling ─────────────────────────────────────────────────

  function scheduleCompletion(step: TourStep): void {
    switch (step.completion_mode) {
      case "auto":
        autoTimer = setTimeout(() => {
          void advanceStep();
        }, step.estimated_seconds * 1000);
        break;

      case "wait_event": {
        const eventName = step.interaction?.validation_event;
        if (eventName !== null && eventName !== undefined) {
          void listen(eventName, () => {
            void advanceStep();
          }).then((fn) => {
            waitEventUnlisten = fn;
          });
        }
        break;
      }

      case "click_next":
      default:
        // Completion is triggered by the user clicking Next.
        break;
    }
  }

  // ─── Navigation actions ─────────────────────────────────────────────────────

  async function advanceStep(): Promise<void> {
    if (skipping) return;

    const step = currentStep;
    if (step === null) return;

    clearAutoTimer();
    clearWaitEvent();

    try {
      await invoke("complete_tour_step", { stepId: step.id });
      await onboardingStore.refreshState();
    } catch (err) {
      console.error("[GuidedTour] complete_tour_step failed:", err);
    }

    if (stepIndex < steps.length - 1) {
      stepIndex += 1;
      await activateStep(stepIndex);
    } else {
      await finishTour();
    }
  }

  async function retreatStep(): Promise<void> {
    if (skipping || stepIndex === 0) return;
    clearAutoTimer();
    clearWaitEvent();
    stepIndex -= 1;
    await activateStep(stepIndex);
  }

  async function finishTour(): Promise<void> {
    skipping = true;
    try {
      await onboardingStore.advancePhase("graduation" as OnboardingPhase);
    } catch (err) {
      console.error("[GuidedTour] advancePhase graduation failed:", err);
    }
  }

  // ─── Escape / exit confirmation ─────────────────────────────────────────────

  function requestExit(): void {
    showConfirmExit = true;
  }

  async function confirmExit(): Promise<void> {
    showConfirmExit = false;
    await finishTour();
  }

  function cancelExit(): void {
    showConfirmExit = false;
  }

  // ─── Keyboard shortcuts ─────────────────────────────────────────────────────

  function handleKeydown(e: KeyboardEvent): void {
    if (showConfirmExit) {
      if (e.key === "Escape") cancelExit();
      if (e.key === "Enter") void confirmExit();
      return;
    }

    switch (e.key) {
      case "ArrowRight":
        e.preventDefault();
        void advanceStep();
        break;
      case "ArrowLeft":
        e.preventDefault();
        void retreatStep();
        break;
      case "Escape":
        e.preventDefault();
        requestExit();
        break;
    }
  }

  // ─── Cleanup helpers ───────────────────────────────────────────────────────

  function clearAutoTimer(): void {
    if (autoTimer !== null) {
      clearTimeout(autoTimer);
      autoTimer = null;
    }
  }

  function clearWaitEvent(): void {
    if (waitEventUnlisten !== null) {
      waitEventUnlisten();
      waitEventUnlisten = null;
    }
  }

  function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
</script>

<svelte:window on:keydown={handleKeydown} />

{#if loading}
  <div class="tour-loading" data-testid="tour-loading">
    <div class="loading-spinner" aria-label="Chargement du tour…"></div>
  </div>
{:else if steps.length === 0}
  <!-- Graceful empty state: no steps returned — skip to graduation. -->
  <div class="tour-loading" data-testid="tour-empty">
    <button class="skip-action-btn" onclick={() => void finishTour()}>
      Continuer →
    </button>
  </div>
{:else}
  {#if currentStep !== null}
    <TourSpotlight
      targetSelector={currentStep.spotlight_selector ?? ""}
      visible={currentStep.spotlight_selector !== null}
      onoverlaclick={requestExit}
    />

    <TourProgressRail {totalSteps} currentStep={stepIndex} />

    <TourStepCard
      title={currentStep.companion_message_key}
      description=""
      stepIndex={stepIndex}
      {totalSteps}
      {targetRect}
      showPrev={stepIndex > 0}
      showNext={true}
      showSkip={true}
      onnext={() => void advanceStep()}
      onprev={() => void retreatStep()}
      onskip={requestExit}
    />
  {/if}

  {#if showConfirmExit}
    <div
      class="confirm-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      data-testid="tour-exit-confirm"
    >
      <div class="confirm-card">
        <h3 id="confirm-title" class="confirm-title">Interrompre le tour ?</h3>
        <p class="confirm-body">
          Votre progression est sauvegardée. Vous pouvez reprendre le tour plus tard.
        </p>
        <div class="confirm-actions">
          <button class="btn-cancel" onclick={cancelExit}>
            Continuer le tour
          </button>
          <button class="btn-confirm" onclick={() => void confirmExit()}>
            Terminer maintenant
          </button>
        </div>
      </div>
    </div>
  {/if}
{/if}

<style>
  .tour-loading {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 248, 240, 0.9);
  }

  .loading-spinner {
    width: 2rem;
    height: 2rem;
    border-radius: 50%;
    border: 2.5px solid rgba(52, 53, 245, 0.2);
    border-top-color: #3435f5;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .skip-action-btn {
    padding: 0.5rem 1.25rem;
    border-radius: 0.5rem;
    border: none;
    background: #3435f5;
    color: #fff;
    font-size: 0.875rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 120ms ease;
  }

  .skip-action-btn:hover {
    opacity: 0.85;
  }

  /* Exit confirmation dialog */
  .confirm-overlay {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.45);
  }

  .confirm-card {
    background: #fff;
    border-radius: 12px;
    padding: 1.75rem;
    width: 360px;
    max-width: calc(100vw - 2rem);
    box-shadow:
      0 0 0 1px rgba(52, 53, 245, 0.08),
      0 16px 48px -8px rgba(0, 0, 0, 0.18);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .confirm-title {
    font-size: 1rem;
    font-weight: 700;
    color: #1a1a2e;
    margin: 0;
  }

  .confirm-body {
    font-size: 0.875rem;
    color: #4b5563;
    margin: 0;
    line-height: 1.55;
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .btn-cancel {
    padding: 0.5rem 0.875rem;
    border-radius: 0.5rem;
    border: 1px solid rgba(0, 0, 0, 0.12);
    background: transparent;
    font-size: 0.8125rem;
    font-weight: 600;
    color: #6b7280;
    cursor: pointer;
    transition: background 120ms ease;
  }

  .btn-cancel:hover {
    background: rgba(0, 0, 0, 0.04);
  }

  .btn-confirm {
    padding: 0.5rem 0.875rem;
    border-radius: 0.5rem;
    border: none;
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    color: #fff;
    font-size: 0.8125rem;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 2px 8px -1px rgba(52, 53, 245, 0.35);
    transition: transform 120ms ease, box-shadow 120ms ease;
  }

  .btn-confirm:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 14px -2px rgba(52, 53, 245, 0.45);
  }
</style>
