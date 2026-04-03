<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn, type Event } from "@tauri-apps/api/event";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { tourPrefill, tourCompanionOverride, tourOpenAgentDetail } from "$lib/stores/tour";
  import { navigateTo } from "$lib/stores/navigation";
  import TourSpotlight from "./TourSpotlight.svelte";
  import TourStepCard from "./TourStepCard.svelte";
  import TourProgressRail from "./TourProgressRail.svelte";
  import VoiceIndicator from "./VoiceIndicator.svelte";
  import type { TourStep, OnboardingPhase, AgentListItem, TourVoiceAction, SttStatus } from "$lib/types";
  import type { Route } from "$lib/stores/navigation";

  // ─── Constants ────────────────────────────────────────────────────────────────

  /** Seconds before an interactive step is auto-skipped. */
  const INTERACTION_TIMEOUT_S = 30;

  // ─── State ─────────────────────────────────────────────────────────────────

  let steps = $state<TourStep[]>([]);
  let stepIndex = $state(0);
  /** i18n namespace prefix derived from the active profile: `"op"` or `"bld"`. */
  let profileNs = $state("op");
  let targetRect = $state<DOMRect | null>(null);
  let loading = $state(true);
  let skipping = $state(false);
  let showConfirmExit = $state(false);
  let autoTimer = $state<ReturnType<typeof setTimeout> | null>(null);
  let timeoutTimer = $state<ReturnType<typeof setTimeout> | null>(null);
  let waitEventUnlisten = $state<UnlistenFn | null>(null);

  // ─── Voice command state ───────────────────────────────────────────────────

  /** True when the STT engine is loaded and the mic button should be shown. */
  let sttAvailable = $state(false);
  /** True while the mic button is held down (recording active). */
  let isVoiceRecording = $state(false);
  /**
   * Set to true on mousedown so the next stt-transcribed event is processed.
   * Cleared after the action is dispatched or recording is cancelled.
   */
  let voiceCommandPending = $state(false);
  let sttTranscribedUnlisten = $state<UnlistenFn | null>(null);

  // ─── Derived helpers ────────────────────────────────────────────────────────

  let currentStep = $derived(steps[stepIndex] ?? null);
  let totalSteps = $derived(steps.length);

  /** Resolved companion title: override key takes precedence over step key. */
  let companionTitle = $derived.by(() => {
    const override = get(tourCompanionOverride);
    if (override !== null) return $t(override);
    if (currentStep !== null) return $t(currentStep.companion_message_key);
    return "";
  });

  // ─── Lifecycle ─────────────────────────────────────────────────────────────

  onMount(() => {
    void loadAndStart();
    void checkSttAvailability();
    void registerSttListener();

    return () => {
      clearAutoTimer();
      clearTimeoutTimer();
      clearWaitEvent();
      clearSttListener();
      tourPrefill.set(null);
      tourCompanionOverride.set(null);
      tourOpenAgentDetail.set(null);
    };
  });

  // ─── STT availability ──────────────────────────────────────────────────────

  async function checkSttAvailability(): Promise<void> {
    try {
      const status = await invoke<SttStatus>("get_stt_status");
      sttAvailable = status.enabled && status.model_loaded;
    } catch {
      // STT engine not available — mic button will not be shown.
      sttAvailable = false;
    }
  }

  async function registerSttListener(): Promise<void> {
    sttTranscribedUnlisten = await listen<string>(
      "stt-transcribed",
      (event: Event<string>) => {
        if (voiceCommandPending) {
          voiceCommandPending = false;
          void dispatchVoiceTranscript(event.payload);
        }
      },
    );
  }

  function clearSttListener(): void {
    if (sttTranscribedUnlisten !== null) {
      sttTranscribedUnlisten();
      sttTranscribedUnlisten = null;
    }
  }

  // ─── Voice push-to-talk ────────────────────────────────────────────────────

  async function handleMicDown(): Promise<void> {
    if (!sttAvailable || isVoiceRecording) return;
    voiceCommandPending = true;
    isVoiceRecording = true;
    try {
      await invoke("start_tour_recording");
    } catch (err) {
      console.warn("[GuidedTour] start_tour_recording failed:", err);
      isVoiceRecording = false;
      voiceCommandPending = false;
    }
  }

  async function handleMicUp(): Promise<void> {
    if (!isVoiceRecording) return;
    isVoiceRecording = false;
    try {
      await invoke("stop_tour_recording");
    } catch (err) {
      console.warn("[GuidedTour] stop_tour_recording failed:", err);
      voiceCommandPending = false;
    }
  }

  async function dispatchVoiceTranscript(transcript: string): Promise<void> {
    let action: TourVoiceAction;
    try {
      action = await invoke<TourVoiceAction>("process_tour_voice_command", {
        transcript,
      });
    } catch (err) {
      console.warn("[GuidedTour] process_tour_voice_command failed:", err);
      return;
    }

    switch (action.action) {
      case "NextStep":
        void advanceStep();
        break;
      case "PreviousStep":
        void retreatStep();
        break;
      case "SkipTour":
        requestExit();
        break;
      case "AskCompanion": {
        const sessionId = get(onboardingStore).companion_session_id;
        if (sessionId !== null && sessionId !== undefined && sessionId !== "") {
          try {
            await invoke("send_chat_message", {
              sessionId,
              content: action.message,
            });
          } catch (err) {
            console.warn("[GuidedTour] send_chat_message failed:", err);
          }
        }
        break;
      }
      case "Unrecognized":
        // Empty transcript — no action.
        break;
    }
  }

  // ─── Load & start ──────────────────────────────────────────────────────────

  async function loadAndStart(): Promise<void> {
    const state = get(onboardingStore);
    const profile = state.profile ?? "operator";
    profileNs = profile === "builder" ? "bld" : "op";

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
    clearTimeoutTimer();
    clearWaitEvent();
    tourCompanionOverride.set(null);

    const step = steps[index];
    if (step === undefined) return;

    // Publish prefilled data for target pages to read.
    tourPrefill.set(step.interaction ?? null);
    // Clear any pending programmatic panel open from the previous step.
    tourOpenAgentDetail.set(null);

    // Navigate to the step's route before DOM operations.
    const route = step.route.replace(/^\//, "") as Route;
    navigateTo(route);

    // Wait for Svelte to flush and the browser to paint.
    await tick();
    await sleep(80);

    // For the agent-detail step, open the detail panel programmatically.
    if (step.id === "bld-agent-detail") {
      tourOpenAgentDetail.set("csv-data-worker");
      // Allow an extra tick for the panel to mount before resolving the selector.
      await tick();
      await sleep(200);
    }

    // Resolve the spotlight target with retry logic.
    targetRect = await resolveSelector(step.spotlight_selector);

    // For the agent-start step, check if the demo agent is already running.
    if (
      step.interaction?.interaction_type === "start_agent" &&
      step.interaction.prefilled_data !== null &&
      step.interaction.prefilled_data !== undefined
    ) {
      const agentId = step.interaction.prefilled_data["agent_id"];
      if (typeof agentId === "string") {
        const alreadyRunning = await isDemoAgentRunning(agentId);
        if (alreadyRunning) {
          tourCompanionOverride.set(`onboarding.tour.${profileNs}.agents.already_active`);
          await advanceStep();
          return;
        }
      }
    }

    // Set up completion based on the mode.
    scheduleCompletion(step);
  }

  // ─── Demo agent pre-check ─────────────────────────────────────────────────

  async function isDemoAgentRunning(agentName: string): Promise<boolean> {
    try {
      const agents = await invoke<AgentListItem[]>("list_agents");
      return agents.some(
        (a) => a.name === agentName && a.runtime_status === "active",
      );
    } catch (err) {
      console.warn("[GuidedTour] list_agents failed, skipping pre-check:", err);
      return false;
    }
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
          // Listen for the validation event on the generic "runtime-event" channel.
          void listen<{ event_type: string }>("runtime-event", (event: Event<{ event_type: string }>) => {
            if (event.payload.event_type === eventName) {
              void advanceStep();
            }
          }).then((fn) => {
            waitEventUnlisten = fn;
          });

          // Auto-skip after INTERACTION_TIMEOUT_S if no event arrives.
          timeoutTimer = setTimeout(() => {
            tourCompanionOverride.set(`onboarding.tour.${profileNs}.timeout_skip`);
            void advanceStep();
          }, INTERACTION_TIMEOUT_S * 1000);
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
    clearTimeoutTimer();
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
    clearTimeoutTimer();
    clearWaitEvent();
    stepIndex -= 1;
    await activateStep(stepIndex);
  }

  async function finishTour(): Promise<void> {
    skipping = true;
    tourPrefill.set(null);
    tourCompanionOverride.set(null);
    tourOpenAgentDetail.set(null);
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

  function clearTimeoutTimer(): void {
    if (timeoutTimer !== null) {
      clearTimeout(timeoutTimer);
      timeoutTimer = null;
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
    <div class="loading-spinner" aria-label={$t("onboarding_v2.tour.loading")}></div>
  </div>
{:else if steps.length === 0}
  <!-- Graceful empty state: no steps returned — skip to graduation. -->
  <div class="tour-loading" data-testid="tour-empty">
    <button class="skip-action-btn" onclick={() => void finishTour()}>
      {$t("onboarding_v2.tour.continue")}
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
      title={companionTitle}
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

    {#if sttAvailable}
      <button
        class="mic-btn"
        class:active={isVoiceRecording}
        data-testid="tour-mic-btn"
        aria-label={isVoiceRecording ? $t("onboarding_v2.tour.mic_recording") : $t("onboarding_v2.tour.mic_hold")}
        onmousedown={() => void handleMicDown()}
        onmouseup={() => void handleMicUp()}
        onmouseleave={() => void handleMicUp()}
      >
        <VoiceIndicator {sttAvailable} isRecording={isVoiceRecording} />
      </button>
    {/if}
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
        <h3 id="confirm-title" class="confirm-title">{$t("onboarding_v2.tour.exit_title")}</h3>
        <p class="confirm-body">
          {$t("onboarding_v2.tour.exit_body")}
        </p>
        <div class="confirm-actions">
          <button class="btn-cancel" onclick={cancelExit}>
            {$t("onboarding_v2.tour.exit_cancel")}
          </button>
          <button class="btn-confirm" onclick={() => void confirmExit()}>
            {$t("onboarding_v2.tour.exit_confirm")}
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

  /* Mic push-to-talk button */
  .mic-btn {
    position: fixed;
    bottom: 1.5rem;
    right: 1.5rem;
    z-index: 60;
    width: 2.75rem;
    height: 2.75rem;
    border-radius: 50%;
    border: none;
    background: rgba(255, 255, 255, 0.92);
    box-shadow:
      0 0 0 1px rgba(52, 53, 245, 0.12),
      0 4px 16px -2px rgba(0, 0, 0, 0.14);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: box-shadow 120ms ease, transform 120ms ease;
    padding: 0;
    user-select: none;
  }

  .mic-btn:hover {
    box-shadow:
      0 0 0 1px rgba(52, 53, 245, 0.2),
      0 6px 20px -2px rgba(0, 0, 0, 0.18);
  }

  .mic-btn.active {
    background: rgba(52, 53, 245, 0.06);
    transform: scale(1.08);
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
