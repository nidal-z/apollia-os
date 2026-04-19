<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn, type Event } from "@tauri-apps/api/event";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { tourPrefill, tourCompanionOverride, tourOpenAgentDetail, tourOpenChatPicker } from "$lib/stores/tour";
  import { navigateTo } from "$lib/stores/navigation";
  import TourSpotlight from "./TourSpotlight.svelte";
  import TourStepCard from "./TourStepCard.svelte";
  import TourProgressRail from "./TourProgressRail.svelte";
  import VoiceIndicator from "./VoiceIndicator.svelte";
  import type { TourStep, OnboardingPhase, TourVoiceAction, SttStatus } from "$lib/types";
  import type { Route } from "$lib/stores/navigation";

  // ─── State ─────────────────────────────────────────────────────────────────

  let steps = $state<TourStep[]>([]);
  let stepIndex = $state(0);
  /** i18n namespace prefix derived from the active profile: `"op"` or `"bld"`. */
  let profileNs = $state("op");
  let targetRect = $state<DOMRect | null>(null);
  /** The CSS selector of the current step's spotlight target, kept in sync for
   *  reactive rect refresh on window resize / scroll. */
  let activeSelector = $state<string | null>(null);
  let loading = $state(true);
  let skipping = $state(false);
  let showConfirmExit = $state(false);
  let autoTimer = $state<ReturnType<typeof setTimeout> | null>(null);
  let timeoutTimer = $state<ReturnType<typeof setTimeout> | null>(null);
  let waitEventUnlisten = $state<UnlistenFn | null>(null);
  /** Generic per-step cleanup callback (DOM listeners, etc.). */
  let stepCleanup = $state<(() => void) | null>(null);

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

  // ─── Group computation ────────────────────────────────────────────────────

  interface GroupInfo {
    label: string;
    startIndex: number;
    endIndex: number;
    count: number;
  }

  function computeGroups(list: TourStep[]): GroupInfo[] {
    const result: GroupInfo[] = [];
    for (let i = 0; i < list.length; i++) {
      const g = list[i].group ?? list[i].id;
      const last = result[result.length - 1];
      if (last && last.label === g) {
        last.endIndex = i;
        last.count++;
      } else {
        result.push({ label: g, startIndex: i, endIndex: i, count: 1 });
      }
    }
    return result;
  }

  let groups = $derived(computeGroups(steps));
  let currentGroup = $derived(
    groups.find((g) => stepIndex >= g.startIndex && stepIndex <= g.endIndex) ?? null
  );
  let groupLabel = $derived(
    currentGroup !== null ? $t(`onboarding_v2.tour.group.${currentGroup.label}`) : ""
  );
  let subStepIndex = $derived(
    currentGroup !== null ? stepIndex - currentGroup.startIndex : 0
  );
  let subStepCount = $derived(currentGroup?.count ?? 1);

  // ─── Lifecycle ─────────────────────────────────────────────────────────────

  /** Re-measure the current spotlight target's bounding rect.
   *  Called on resize / scroll so the card and spotlight stay aligned. */
  function refreshTargetRect(): void {
    if (activeSelector === null) {
      targetRect = null;
      return;
    }
    const el = document.querySelector(activeSelector);
    targetRect = el !== null ? el.getBoundingClientRect() : null;
  }

  let resizeDebounce: ReturnType<typeof setTimeout> | null = null;
  function onResizeOrScroll(): void {
    if (resizeDebounce !== null) clearTimeout(resizeDebounce);
    resizeDebounce = setTimeout(refreshTargetRect, 60);
  }

  // ─── Layout-settle tracking ───────────────────────────────────────────────
  // After navigation the target element may shift as CSS transitions, lazy
  // content, and sidebar animations settle.  We poll via rAF for up to 1.5 s,
  // updating targetRect each time the element moves.

  let settleRaf: number | null = null;

  function stopSettle(): void {
    if (settleRaf !== null) {
      cancelAnimationFrame(settleRaf);
      settleRaf = null;
    }
  }

  function startSettle(): void {
    stopSettle();
    const startTime = performance.now();
    const SETTLE_DURATION_MS = 1500;

    function poll(): void {
      refreshTargetRect();
      if (performance.now() - startTime < SETTLE_DURATION_MS) {
        settleRaf = requestAnimationFrame(poll);
      } else {
        settleRaf = null;
      }
    }

    settleRaf = requestAnimationFrame(poll);
  }

  onMount(() => {
    void loadAndStart();
    void checkSttAvailability();
    void registerSttListener();

    window.addEventListener("resize", onResizeOrScroll);
    window.addEventListener("scroll", onResizeOrScroll, { passive: true });

    return () => {
      clearAutoTimer();
      clearTimeoutTimer();
      clearWaitEvent();
      clearSttListener();
      stopSettle();
      if (stepCleanup) { stepCleanup(); stepCleanup = null; }
      window.removeEventListener("resize", onResizeOrScroll);
      window.removeEventListener("scroll", onResizeOrScroll);
      if (resizeDebounce !== null) clearTimeout(resizeDebounce);
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
    // tour_step_index is the *next* step to show (incremented on completion),
    // so it already points to the first uncompleted step.  However, if it
    // exceeds the last valid index the user has already finished the tour —
    // clamp to the last step so the graduation auto-advance can fire.
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
    stopSettle();
    if (stepCleanup) { stepCleanup(); stepCleanup = null; }
    tourCompanionOverride.set(null);
    document.querySelectorAll(".tour-ring-pulse").forEach((el) => el.classList.remove("tour-ring-pulse"));

    const step = steps[index];
    if (step === undefined) return;

    tourPrefill.set(step.interaction ?? null);
    tourOpenAgentDetail.set(null);

    // Navigate to the step's route.
    const route = step.route.replace(/^\//, "") as Route;
    navigateTo(route);
    await tick();
    await sleep(80);

    // Run step-specific pre-activation hook.
    const cleanup = await runStepHook(step);
    if (cleanup) stepCleanup = cleanup;

    // Resolve the spotlight target.
    activeSelector = step.spotlight_selector ?? null;
    targetRect = await resolveSelector(step.spotlight_selector);
    startSettle();

    // Run step-specific post-resolve hook (ring-pulse, etc.).
    await runPostResolveHook(step);

    // Set up completion based on the mode.
    scheduleCompletion(step);
  }

  // ─── Step hooks ───────────────────────────────────────────────────────────

  /** Pre-activation hook: runs before selector resolution. Returns optional cleanup. */
  async function runStepHook(step: TourStep): Promise<(() => void) | null> {
    switch (step.id) {
      // Builder: open agent detail panel
      case "bld-agent-detail":
        tourOpenAgentDetail.set("csv-data-worker");
        await tick();
        await sleep(200);
        return null;

      // Operator: stop onboarding-agent so user sees Start button
      case "op-agents-2":
        await ensureAgentStopped("onboarding-agent");
        await tick();
        await sleep(300);
        return null;

      // Operator: listen for click on "New Chat" → auto-advance
      case "op-chat-1": {
        await tick();
        const btn = document.querySelector('[data-testid="new-chat-button"]');
        if (btn) {
          const handler = () => void advanceStep();
          btn.addEventListener("click", handler, { once: true });
          return () => btn.removeEventListener("click", handler);
        }
        return null;
      }

      // Operator: open the chat picker programmatically
      case "op-chat-2":
        tourOpenChatPicker.set(true);
        await tick();
        await sleep(200);
        return null;

      // Operator: create a demo trigger so the page isn't empty
      case "op-triggers-1":
        try {
          await invoke("create_trigger", {
            definition: {
              id: "tour-demo-trigger",
              agent: "onboarding-agent",
              enabled: true,
              on_busy: "queue",
              source: { type: "interval", every: "5m" },
            },
          });
        } catch {
          // Trigger may already exist from a previous tour run — ignore.
        }
        await tick();
        await sleep(300);
        return null;

      // Operator: create a demo notification channel
      case "op-notifications-1":
        try {
          await invoke("create_notification_channel", {
            channel: {
              id: "tour-alerts",
              channel_type: "desktop",
              enabled: true,
              config: {},
              events: null,
            },
          });
        } catch {
          // Channel may already exist — ignore.
        }
        // Give the backend time to persist the channel before the page renders.
        await sleep(500);
        // Re-navigate to force the page to reload its channel list.
        navigateTo("notifications" as Route);
        await tick();
        await sleep(300);
        return null;

      // Operator: send a test notification (with retry for propagation delay)
      case "op-notifications-2":
        for (let attempt = 0; attempt < 3; attempt++) {
          try {
            await invoke("test_notification_channel", { channelId: "tour-alerts" });
            tourCompanionOverride.set("onboarding.tour.op.notifications_test.success");
            setTimeout(() => tourCompanionOverride.set(null), 3000);
            break;
          } catch {
            if (attempt < 2) await sleep(500);
          }
        }
        return null;

      default:
        return null;
    }
  }

  /** Post-resolve hook: runs after selector is resolved (for ring-pulse, etc.). */
  async function runPostResolveHook(step: TourStep): Promise<void> {
    switch (step.id) {
      // Operator: ring-pulse on Start button
      case "op-agents-2": {
        await tick();
        const btn = document.querySelector(
          '[data-agent-name="onboarding-agent"] [data-testid="agent-start-btn"]'
        );
        if (btn) btn.classList.add("tour-ring-pulse");
        break;
      }

      // Operator: ring-pulse on "Start Libre" button
      case "op-chat-3": {
        await tick();
        const btn = document.querySelector('[data-testid="pick-libre"]');
        if (btn) btn.classList.add("tour-ring-pulse");
        break;
      }
    }
  }

  // ─── Agent stop helper ─────────────────────────────────────────────────────

  /** Stops an agent by name so the user can re-activate it during the tour. */
  async function ensureAgentStopped(agentName: string): Promise<void> {
    try {
      const agents = await invoke<{ name: string; id: string | null; runtime_status: string | null }[]>("list_agents");
      const agent = agents.find((a) => a.name === agentName);
      if (agent?.id && (agent.runtime_status === "active" || agent.runtime_status === "degraded")) {
        await invoke("stop_agent", { agentId: agent.id });
      }
    } catch (err) {
      console.warn(`[GuidedTour] ensureAgentStopped(${agentName}) failed:`, err);
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
        // "auto" steps still wait for the user to click Next.  The
        // estimated_seconds field is informational only — we never
        // auto-advance because the user needs time to read the content
        // and the actual reading time varies with window size, language, etc.
        break;

      case "wait_event": {
        const eventName = step.interaction?.validation_event;
        if (eventName !== null && eventName !== undefined) {
          const successKey = `${step.companion_message_key.replace('.title', '.success')}`;
          void listen<{ event_type: string }>("runtime-event", (event: Event<{ event_type: string }>) => {
            if (event.payload.event_type === eventName) {
              // Show success message briefly, then advance.
              tourCompanionOverride.set(successKey);
              setTimeout(() => {
                tourCompanionOverride.set(null);
                void advanceStep();
              }, 2000);
            }
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

    <TourProgressRail {steps} currentStep={stepIndex} />

    <TourStepCard
      title={companionTitle}
      description=""
      stepIndex={stepIndex}
      {totalSteps}
      {targetRect}
      {groupLabel}
      {subStepIndex}
      {subStepCount}
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
    background: hsl(var(--background) / 0.9);
  }

  .loading-spinner {
    width: 2rem;
    height: 2rem;
    border-radius: 50%;
    border: 2.5px solid hsl(var(--primary) / 0.2);
    border-top-color: hsl(var(--primary));
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .skip-action-btn {
    padding: 0.5rem 1.25rem;
    border-radius: 0.5rem;
    border: none;
    background: hsl(var(--primary));
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
    background: hsl(var(--card) / 0.92);
    box-shadow: var(--shadow-elev-2);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: box-shadow 120ms ease, transform 120ms ease;
    padding: 0;
    user-select: none;
  }

  .mic-btn:hover {
    box-shadow: var(--shadow-elev-3);
  }

  .mic-btn.active {
    background: hsl(var(--primary) / 0.06);
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
    box-shadow: var(--shadow-elev-4);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .confirm-title {
    font-size: 1rem;
    font-weight: 700;
    color: hsl(var(--foreground));
    margin: 0;
  }

  .confirm-body {
    font-size: 0.875rem;
    color: hsl(var(--foreground) / 0.8);
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
    color: hsl(var(--muted-foreground));
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
    background: linear-gradient(135deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)));
    color: hsl(var(--primary-foreground));
    font-size: 0.8125rem;
    font-weight: 600;
    cursor: pointer;
    box-shadow: var(--shadow-primary-sm);
    transition: transform 120ms ease, box-shadow 120ms ease;
  }

  .btn-confirm:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-primary-md);
  }
</style>
