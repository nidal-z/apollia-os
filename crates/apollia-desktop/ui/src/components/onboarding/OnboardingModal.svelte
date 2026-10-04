<script lang="ts">
  /**
   * Onboarding orchestrator (multi-step modal).
   *
   * Drives the new-user flow as a sequence of dedicated screens, each
   * implemented as a self-contained component:
   *
   *   1. {@link OnboardingWelcome}  - intro and consent to start
   *   2. {@link OnboardingAiSetup}  - local LLM (GGUF) + STT (Whisper)
   *   3. {@link OnboardingChatStep} - a short agent-driven chat, then the
   *                                   proposed profile read over before it
   *                                   is saved
   *
   * The list itself is `ONBOARDING_STEPS` in `onboardingSteps.ts`. No step asks
   * for a usage profile: the developer mode is a setting, off by default.
   *
   * The phase machine in `apollia_runtime::onboarding` is updated in lockstep via `advance_onboarding_phase`, so the backend
   * still owns analytics/persistence even though navigation is local.
   *
   * This is a first-run vitrine surface: the card entrance, the signature
   * gradient hairline under the header, the spring-loaded stepper, and the
   * directional step swap lean on the expressive-depth tokens. All motion
   * collapses under reduced-motion.
   */
  import { onMount, untrack } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t } from "svelte-i18n";
  import { duration, rm } from "$lib/design/motion";
  import {
    advanceOnboardingPhase,
    dismissOnboarding,
    getOnboardingState,
  } from "$lib/ipc/onboarding";
  import { skipOnboarding } from "$lib/onboarding/skipOnboarding";
  import { get } from "svelte/store";
  import { profileEnrichment } from "$lib/stores/onboarding";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { ErrorBanner } from "$lib/components/operator";
  import OnboardingWelcome from "./OnboardingWelcome.svelte";
  import OnboardingAiSetup from "./OnboardingAiSetup.svelte";
  import OnboardingChatStep from "./OnboardingChatStep.svelte";
  import {
    ONBOARDING_STEPS,
    phaseOfStep,
    stepOfPhase,
    type OnboardingStep,
  } from "./onboardingSteps";
  import type { OnboardingPhase } from "$lib/types";

  interface Props {
    onclose: () => void;
  }

  const { onclose }: Props = $props();

  type Step = OnboardingStep;

  const STEPS = ONBOARDING_STEPS;

  let currentStep = $state<Step>("welcome");
  let navDir = $state<1 | -1>(1);
  let unlistenRuntime: UnlistenFn | null = null;
  let rootEl = $state<HTMLDivElement | null>(null);
  // Bumped by the footer during the chat step to ask the chat to skip the
  // questions and open the profile review.
  let chatSkipSignal = $state(0);
  // Why inline and not a toast: the overlay sits at `z-index: 80` and the
  // toast container at `--z-toast: 40` (`app.css`), so a toast fired from
  // inside this modal is painted underneath it.
  let skipError = $state<HumanizedError | null>(null);
  // Set once the conversation reports it never started: Escape becomes an exit
  // again, because there is no turn left to interrupt by accident.
  let chatStranded = $state(false);
  // Opened by Settings > Profile to complete a profile, not by a first launch:
  // the modal goes straight to the conversation and leaves the first-launch
  // state (phase, skipped flag) alone.
  const enriching = get(profileEnrichment) !== null;

  const stepIndex = $derived(STEPS.findIndex((s) => s.id === currentStep));

  // The chat is the only step that absolutely demands a focus trap that
  // swallows Escape (otherwise users may dismiss the agent dialog mid-turn
  // by accident). Earlier steps allow the standard Escape to skip.
  function captureEscape(event: KeyboardEvent): void {
    if (currentStep === "chat" && !chatStranded && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
    }
  }

  // Best-effort backend phase sync. The phase machine validates strict
  // transitions (Welcome -> AiSetup -> Acquaintance -> ...),
  // so we map each frontend step to the matching legal target.
  async function syncBackendPhase(target: OnboardingPhase): Promise<void> {
    try {
      await advanceOnboardingPhase(target);
    } catch {
      // Non-blocking: the phase machine may already be ahead (e.g. user
      // restarted onboarding from settings). Don't surface to the user.
    }
  }

  function goTo(step: Step): void {
    navDir = STEPS.findIndex((s) => s.id === step) >= stepIndex ? 1 : -1;
    currentStep = step;
    untrack(() => {
      const target = phaseOfStep(step);
      if (target) void syncBackendPhase(target);
    });
  }

  // Skipping is what writes `onboarding_skipped`, and that flag is the one
  // `App.svelte` re-reads at launch to decide whether this modal opens again.
  // Closing on a failed write claimed a skip the backend never recorded.
  async function handleSkip(): Promise<void> {
    skipError = null;
    await skipOnboarding({
      dismiss: dismissOnboarding,
      report: (err) => {
        skipError = reportError(err, { surface: "inline" });
      },
      close: onclose,
    });
  }

  // On mount, snap to the step that matches the persisted backend phase.
  // This makes the flow resumable across app restarts and after users
  // bounce out to /llm to add a cloud backend.
  async function restoreStepFromBackend(): Promise<void> {
    if (enriching) {
      currentStep = "chat";
      return;
    }
    try {
      const state = await getOnboardingState();
      currentStep = stepOfPhase(state.phase);
    } catch {
      // Backend unreachable - keep the default "welcome" step.
    }
  }

  onMount(() => {
    rootEl?.focus();
    void restoreStepFromBackend();

    void listen<{ category: string; event_type: string }>(
      "runtime-event",
      (evt) => {
        if (evt.payload.category !== "onboarding-changed") return;
        if (evt.payload.event_type === "OnboardingCompleted") {
          onclose();
        }
      },
    ).then((fn) => {
      unlistenRuntime = fn;
    });

    return () => {
      unlistenRuntime?.();
      // A request the conversation never took (the modal was closed before
      // it started) must not turn the next first-launch run into a resume.
      profileEnrichment.set(null);
    };
  });
</script>

<svelte:window onkeydown={captureEscape} />

<div
  bind:this={rootEl}
  tabindex="-1"
  role="dialog"
  aria-modal="true"
  aria-labelledby="onboarding-modal-title"
  class="onboarding-overlay"
  data-testid="onboarding-modal"
>
  <div
    class="onboarding-card"
    class:onboarding-card-tall={currentStep === "ai-setup" || currentStep === "chat"}
  >
    <header class="onboarding-card-header">
      <h2 id="onboarding-modal-title" class="onboarding-card-title">
        {$t("onboarding_modal.title")}
      </h2>
      <ol class="onboarding-steps" aria-label={$t("onboarding_modal.progress_label")}>
        {#each STEPS as step, i (step.id)}
          <li
            class="onboarding-step"
            class:done={i < stepIndex}
            class:active={i === stepIndex}
            aria-current={i === stepIndex ? "step" : undefined}
            title={$t(step.labelKey)}
          >
            <span class="onboarding-step-dot">{i + 1}</span>
            <span class="onboarding-step-label">{$t(step.labelKey)}</span>
          </li>
        {/each}
      </ol>
    </header>

    <div
      class="onboarding-card-body"
      class:onboarding-card-body-flush={currentStep === "chat"}
    >
      {#key currentStep}
        <div
          class="onboarding-panel"
          class:onboarding-panel-flush={currentStep === "chat"}
          in:fly={rm({ x: 12 * navDir, duration: duration.base, easing: cubicOut })}
          out:fly={rm({ x: -12 * navDir, duration: duration.fast, easing: cubicOut })}
        >
          {#if currentStep === "welcome"}
            <OnboardingWelcome onnext={() => goTo("ai-setup")} />
          {:else if currentStep === "ai-setup"}
            <OnboardingAiSetup
              onnext={() => goTo("chat")}
              onback={() => goTo("welcome")}
              onskip={() => goTo("chat")}
              onopencloud={onclose}
            />
          {:else if currentStep === "chat"}
            <OnboardingChatStep
              onback={() => goTo("ai-setup")}
              skipSignal={chatSkipSignal}
              onstranded={() => {
                chatStranded = true;
                if (enriching) onclose();
                else void handleSkip();
              }}
              {onclose}
            />
          {/if}
        </div>
      {/key}
    </div>

    {#if skipError}
      <div class="onboarding-card-notice">
        <ErrorBanner data-testid="onboarding-skip-error">
          <p class="font-medium">{skipError.friendly_message}</p>
          <p class="text-caption opacity-90">{skipError.suggested_action}</p>
          {#if skipError.detail}
            <details class="mt-2">
              <summary
                class="cursor-pointer text-caption opacity-70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                data-testid="onboarding-skip-error-details"
              >
                {$t("errors.show_details")}
              </summary>
              <p class="mt-1.5 break-all font-mono text-code-sm opacity-80">
                {skipError.detail}
              </p>
            </details>
          {/if}
        </ErrorBanner>
      </div>
    {/if}

    <footer class="onboarding-card-footer">
      {#if currentStep === "chat"}
        <!-- During the conversation every question is optional: skipping them
             opens the profile review, where the person fills the form by hand
             or leaves without saving. -->
        <button
          type="button"
          class="onboarding-skip"
          data-testid="onboarding-skip-optional"
          onclick={() => (chatSkipSignal += 1)}
        >
          {$t("onboarding_modal.skip_optional")}
        </button>
      {:else}
        <button
          type="button"
          class="onboarding-skip"
          data-testid="onboarding-skip"
          onclick={handleSkip}
        >
          {$t("onboarding_modal.skip_later")}
        </button>
      {/if}
    </footer>
  </div>
</div>

<style>
  /* The top bar and the detail sidebar rise above this veil while it is
     shown (`--z-onboarding-above`), so the help icon and the guide stay
     reachable during the onboarding. */
  .onboarding-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-onboarding);
    display: flex;
    align-items: center;
    justify-content: center;
    background: hsl(var(--background) / 0.72);
    backdrop-filter: blur(9px);
    padding: 1.5rem;
  }

  .onboarding-card {
    position: relative;
    width: 100%;
    max-width: 720px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    background: hsl(var(--card));
    border: 1px solid hsl(var(--border));
    border-radius: 1rem;
    /* Vitrine depth: modal elevation plus a faint signature-gradient rim. */
    box-shadow: var(--shadow-elev-4), 0 0 0 1px hsl(var(--grad-b) / 0.06);
    overflow: hidden;
    animation: onboarding-card-in var(--motion-base) var(--ease-apple) both;
  }

  @keyframes onboarding-card-in {
    from {
      opacity: 0;
      transform: translateY(10px) scale(0.975);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  /* AI setup and chat need extra height - give them a fixed minimum so the
     scroll area is comfortable on small viewports. */
  .onboarding-card-tall {
    height: min(86vh, 760px);
  }

  .onboarding-card-header {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.875rem 1.25rem;
    border-bottom: 1px solid hsl(var(--border) / 0.7);
    flex-shrink: 0;
  }

  /* Signature gradient hairline under the header (hero surface leans richer). */
  .onboarding-card-header::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 1.5px;
    background: linear-gradient(
      90deg,
      hsl(var(--grad-a) / 0.6),
      hsl(var(--grad-b) / 0.3) 36%,
      transparent 76%
    );
  }

  .onboarding-card-title {
    margin: 0;
    font-size: var(--text-heading-sm);
    font-weight: 600;
    color: hsl(var(--foreground));
    letter-spacing: -0.01em;
  }

  .onboarding-steps {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .onboarding-step {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: var(--text-caption);
    color: hsl(var(--muted-foreground));
    transition: color var(--motion-fast) ease;
  }

  .onboarding-step-dot {
    width: 1.375rem;
    height: 1.375rem;
    border-radius: 999px;
    border: 1px solid hsl(var(--border));
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: var(--text-caption);
    font-weight: 600;
    transition:
      background 180ms var(--ease-spring),
      color var(--motion-fast) ease,
      border-color var(--motion-fast) ease,
      box-shadow 180ms var(--ease-spring);
  }

  .onboarding-step-label {
    font-weight: 500;
  }

  /* Hide step labels on narrow widths to keep the rail compact. */
  @media (max-width: 540px) {
    .onboarding-step-label {
      display: none;
    }
  }

  .onboarding-step.active {
    color: hsl(var(--foreground));
  }
  .onboarding-step.active .onboarding-step-dot {
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    border-color: hsl(var(--primary));
    box-shadow: 0 0 0 4px hsl(var(--primary) / 0.14);
  }

  .onboarding-step.done {
    color: hsl(var(--muted-foreground));
  }
  .onboarding-step.done .onboarding-step-dot {
    background: hsl(var(--primary) / 0.18);
    color: hsl(var(--primary));
    border-color: hsl(var(--primary) / 0.35);
  }

  .onboarding-card-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 1.25rem 1.5rem;
  }

  .onboarding-card-body-flush {
    padding: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .onboarding-panel {
    /* Wrapper for the directional step-swap transition. */
    display: block;
  }
  .onboarding-panel-flush {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
  }

  .onboarding-card-notice {
    padding: 0 1.25rem 0.625rem;
    flex-shrink: 0;
  }

  .onboarding-card-footer {
    display: flex;
    justify-content: center;
    padding: 0.625rem 1.25rem 0.875rem;
    border-top: 1px solid hsl(var(--border) / 0.7);
    flex-shrink: 0;
  }

  .onboarding-skip {
    background: none;
    border: none;
    color: hsl(var(--muted-foreground));
    font-size: var(--text-body-xs);
    cursor: pointer;
    padding: 0.25rem 0.75rem;
    border-radius: calc(var(--radius) - 4px);
    transition:
      color var(--motion-fast) ease,
      background var(--motion-fast) ease;
  }

  .onboarding-skip:hover {
    color: hsl(var(--foreground));
    background: hsl(var(--muted));
  }
  .onboarding-skip:focus-visible {
    outline: 2px solid hsl(var(--primary));
    outline-offset: 2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .onboarding-card {
      animation: none;
    }
    .onboarding-step-dot {
      transition: none;
    }
  }
</style>
