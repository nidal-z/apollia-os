<script lang="ts">
  /**
   * Onboarding orchestrator (multi-step modal).
   *
   * Drives the new-user flow as a sequence of dedicated screens, each
   * implemented as a self-contained component:
   *
   *   1. {@link OnboardingWelcome}  - the language of the interface
   *   2. {@link OnboardingAiSetup}  - the language model, the only step that
   *                                   cannot be passed
   *   3. {@link OnboardingEmbeddingsStep} - the model search by meaning
   *                                   uses, recommended for this machine
   *   4. {@link OnboardingChatStep} - a short agent-driven chat, then the
   *                                   proposed profile read over before it
   *                                   is saved
   *
   * The list itself is `ONBOARDING_STEPS` in `onboardingSteps.ts`, and the
   * progress counts that list: a step is announced once it is built. No step
   * asks for a usage profile: the developer mode is a setting, off by default.
   *
   * The header carries what holds on every step: the progress, "configure
   * later", and the download of the language model, which goes on while the
   * steps after the AI setup are done.
   *
   * The phase machine in `apollia_runtime::onboarding` has one phase per step
   * and only moves from a phase to the next, so each move of the modal walks
   * the phases between the one stored and the one of the step that opens. A
   * restart then reopens exactly on the step that was left.
   *
   * The card entrance and the directional step swap lean on the
   * expressive-depth tokens. All motion collapses under reduced-motion.
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
  import { AlertCircle, Check, Download } from "lucide-svelte";
  import OnboardingWelcome from "./OnboardingWelcome.svelte";
  import OnboardingAiSetup from "./OnboardingAiSetup.svelte";
  import OnboardingEmbeddingsStep from "./OnboardingEmbeddingsStep.svelte";
  import OnboardingChatStep from "./OnboardingChatStep.svelte";
  import { downloadNotice } from "./llmDownload";
  import { llmDownload } from "./llmDownloadLive";
  import {
    ONBOARDING_STEPS,
    phaseOfStep,
    phasePath,
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
  // Size of the language model wired at the AI step, which the embeddings
  // step sizes its recommendation beside; `null` leaves the runtime to count
  // the largest language model installed.
  let languageModelBytes = $state<number | null>(null);
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
  const modelNotice = $derived(downloadNotice($llmDownload));

  // The phase the backend stands on, and the chain of moves asked of it. One
  // chain, so two quick moves of the modal reach the backend in their order.
  let backendPhase = "welcome";
  let backendSync: Promise<void> = Promise.resolve();

  // The chat is the only step that absolutely demands a focus trap that
  // swallows Escape (otherwise users may dismiss the agent dialog mid-turn
  // by accident). Earlier steps allow the standard Escape to skip.
  function captureEscape(event: KeyboardEvent): void {
    if (currentStep === "chat" && !chatStranded && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
    }
  }

  // Best-effort backend phase sync. The phase machine only moves from one
  // phase to the next, so every phase between the stored one and the target
  // is walked in order.
  function syncBackendPhase(target: OnboardingPhase): void {
    backendSync = backendSync.then(async () => {
      for (const phase of phasePath(backendPhase, target)) {
        try {
          await advanceOnboardingPhase(phase);
          backendPhase = phase;
        } catch {
          // Non-blocking: the backend was unreachable, or the onboarding was
          // reset under the modal. Navigation stays local; nothing to show.
          return;
        }
      }
    });
  }

  function goTo(step: Step): void {
    navDir = STEPS.findIndex((s) => s.id === step) >= stepIndex ? 1 : -1;
    currentStep = step;
    // A profile completed from the settings leaves the first-launch phase alone.
    if (enriching) return;
    untrack(() => syncBackendPhase(phaseOfStep(step)));
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
      backendPhase = state.phase;
      currentStep = stepOfPhase(state.phase);
    } catch {
      // Backend unreachable - keep the default "welcome" step.
    }
  }

  onMount(() => {
    rootEl?.focus();
    backendSync = restoreStepFromBackend();

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
  aria-label={$t("onboarding_modal.title")}
  class="onboarding-overlay"
  data-testid="onboarding-modal"
>
  <div
    class="onboarding-card"
    class:onboarding-card-tall={currentStep === "ai-setup" || currentStep === "chat"}
  >
    <header class="onboarding-card-header">
      <div class="onboarding-card-bar">
        <p class="onboarding-step-count" data-testid="onboarding-step-count">
          {$t("onboarding_modal.step_count", {
            values: {
              current: stepIndex + 1,
              total: STEPS.length,
              label: $t(STEPS[stepIndex].labelKey),
            },
          })}
        </p>
        <div class="onboarding-card-actions">
          {#if modelNotice}
            <!-- The download goes on while the next steps are done: what it
                 has reached stays in sight on each of them. -->
            <span
              class="onboarding-model-notice"
              class:is-ready={modelNotice.tone === "ready"}
              class:is-failed={modelNotice.tone === "failed"}
              role="status"
              data-testid="onboarding-model-progress"
            >
              {#if modelNotice.tone === "ready"}
                <Check size={12} strokeWidth={2.5} aria-hidden="true" />
              {:else if modelNotice.tone === "failed"}
                <AlertCircle size={12} strokeWidth={2} aria-hidden="true" />
              {:else}
                <Download size={12} strokeWidth={2} aria-hidden="true" />
              {/if}
              {$t(modelNotice.key, { values: modelNotice.values })}
            </span>
          {/if}
          <button
            type="button"
            class="onboarding-skip"
            data-testid="onboarding-skip"
            onclick={handleSkip}
          >
            {$t("onboarding_modal.skip_later")}
          </button>
        </div>
      </div>
      <ol
        class="onboarding-steps"
        aria-label={$t("onboarding_modal.progress_label")}
        data-testid="onboarding-progress"
      >
        {#each STEPS as step, i (step.id)}
          <li
            class="onboarding-step"
            class:done={i < stepIndex}
            class:active={i === stepIndex}
            aria-current={i === stepIndex ? "step" : undefined}
          >
            <span class="onboarding-step-bar" aria-hidden="true"></span>
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
              onnext={(bytes) => {
                languageModelBytes = bytes;
                goTo("embeddings");
              }}
              onback={() => goTo("welcome")}
              onopencloud={onclose}
            />
          {:else if currentStep === "embeddings"}
            <OnboardingEmbeddingsStep
              {languageModelBytes}
              onnext={() => goTo("chat")}
              onback={() => goTo("ai-setup")}
            />
          {:else if currentStep === "chat"}
            <OnboardingChatStep
              onback={() => goTo("embeddings")}
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

    {#if currentStep === "chat"}
      <!-- During the conversation every question is optional: skipping them
           opens the profile review, where the person fills the form by hand
           or leaves without saving. -->
      <footer class="onboarding-card-footer">
        <button
          type="button"
          class="onboarding-skip"
          data-testid="onboarding-skip-optional"
          onclick={() => (chatSkipSignal += 1)}
        >
          {$t("onboarding_modal.skip_optional")}
        </button>
      </footer>
    {/if}
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
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    padding: 0.875rem 1.5rem 0.75rem;
    border-bottom: 1px solid hsl(var(--border) / 0.7);
    flex-shrink: 0;
  }

  .onboarding-card-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .onboarding-step-count {
    margin: 0;
    font-size: var(--text-caption);
    color: hsl(var(--muted-foreground));
  }

  .onboarding-card-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .onboarding-model-notice {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: var(--text-caption);
    color: hsl(var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }
  .onboarding-model-notice.is-ready {
    color: hsl(var(--text-success));
  }
  .onboarding-model-notice.is-failed {
    color: hsl(var(--text-danger));
  }

  .onboarding-steps {
    display: flex;
    align-items: flex-start;
    gap: 0.375rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .onboarding-step {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    font-size: var(--text-caption);
    color: hsl(var(--muted-foreground));
    transition: color var(--motion-fast) ease;
  }

  .onboarding-step-bar {
    height: 3px;
    border-radius: 999px;
    background: hsl(var(--border));
    transition: background var(--motion-fast) ease;
  }

  .onboarding-step-label {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .onboarding-step.active .onboarding-step-bar,
  .onboarding-step.done .onboarding-step-bar {
    background: hsl(var(--primary));
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
    .onboarding-step,
    .onboarding-step-bar {
      transition: none;
    }
  }
</style>
