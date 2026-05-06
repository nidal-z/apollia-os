<script lang="ts">
  /**
   * Onboarding orchestrator (multi-step modal).
   *
   * Drives the new-user flow as a sequence of dedicated screens, each
   * implemented as a self-contained component:
   *
   *   1. {@link OnboardingWelcome}        — intro and consent to start
   *   2. {@link OnboardingProfileSelector} — operator vs builder
   *   3. {@link OnboardingAiSetup}         — local LLM (GGUF) + STT (Whisper)
   *   4. {@link OnboardingChatStep}        — agent-driven chat (4 turns,
   *                                          collects user.name / user.role /
   *                                          HITL / sovereignty)
   *
   * The phase machine in `crates/apollia-desktop/src/commands/onboarding.rs`
   * is updated in lockstep via `advance_onboarding_phase`, so the backend
   * still owns analytics/persistence even though navigation is local.
   *
   * The chat step needs at least one LLM backend to be usable. The AI Setup
   * gate prevents reaching it without one — but we keep an extra safety net
   * inside the chat step (it surfaces the LLM-unavailable state explicitly).
   */
  import { onMount, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import OnboardingWelcome from "./OnboardingWelcome.svelte";
  import OnboardingProfileSelector from "./OnboardingProfileSelector.svelte";
  import OnboardingAiSetup from "./OnboardingAiSetup.svelte";
  import OnboardingChatStep from "./OnboardingChatStep.svelte";
  import type { OnboardingPhase, OnboardingState } from "$lib/types";

  interface Props {
    onclose: () => void;
  }

  const { onclose }: Props = $props();

  type Step = "welcome" | "profile" | "ai-setup" | "chat";

  const STEPS: { id: Step; label: string }[] = [
    { id: "welcome", label: "Accueil" },
    { id: "profile", label: "Profil" },
    { id: "ai-setup", label: "Modèles" },
    { id: "chat", label: "Calibrage" },
  ];

  let currentStep = $state<Step>("welcome");
  let unlistenRuntime: UnlistenFn | null = null;
  let rootEl = $state<HTMLDivElement | null>(null);

  const stepIndex = $derived(STEPS.findIndex((s) => s.id === currentStep));

  // The chat is the only step that absolutely demands a focus trap that
  // swallows Escape (otherwise users may dismiss the agent dialog mid-turn
  // by accident). Earlier steps allow the standard Escape to skip.
  function captureEscape(event: KeyboardEvent): void {
    if (currentStep === "chat" && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
    }
  }

  // Best-effort backend phase sync. The phase machine validates strict
  // transitions (Welcome → ProfileChoice → AiSetup → Acquaintance → …),
  // so we map each frontend step to the matching legal target.
  async function syncBackendPhase(target: OnboardingPhase): Promise<void> {
    try {
      await invoke("advance_onboarding_phase", { targetPhase: target });
    } catch {
      // Non-blocking: the phase machine may already be ahead (e.g. user
      // restarted onboarding from settings). Don't surface to the user.
    }
  }

  function goTo(step: Step): void {
    currentStep = step;
    untrack(() => {
      // Translate frontend step id → backend phase id.
      const phaseMap: Record<Step, OnboardingPhase | null> = {
        welcome: null,           // initial state, nothing to advance to
        profile: "profile_choice",
        "ai-setup": "ai_setup",
        chat: "acquaintance",
      };
      const target = phaseMap[step];
      if (target) void syncBackendPhase(target);
    });
  }

  async function handleSkip(): Promise<void> {
    try {
      await invoke("dismiss_onboarding");
    } catch {
      // best-effort; closing anyway
    }
    onclose();
  }

  // On mount, snap to the step that matches the persisted backend phase.
  // This makes the flow resumable across app restarts and after users
  // bounce out to /llm to add a cloud backend.
  async function restoreStepFromBackend(): Promise<void> {
    try {
      const state = await invoke<OnboardingState>("get_onboarding_state");
      const phaseToStep: Record<OnboardingPhase, Step> = {
        welcome: "welcome",
        profile_choice: "profile",
        ai_setup: "ai-setup",
        acquaintance: "chat",
        guided_tour: "chat",
        graduation: "chat",
        done: "chat",
      };
      const target = phaseToStep[state.phase] ?? "welcome";
      currentStep = target;
    } catch {
      // Backend unreachable — keep the default "welcome" step.
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
        Apollia — Premier contact
      </h2>
      <ol class="onboarding-steps" aria-label="Progression de l'onboarding">
        {#each STEPS as step, i (step.id)}
          <li
            class="onboarding-step"
            class:done={i < stepIndex}
            class:active={i === stepIndex}
            aria-current={i === stepIndex ? "step" : undefined}
            title={step.label}
          >
            <span class="onboarding-step-dot">{i + 1}</span>
            <span class="onboarding-step-label">{step.label}</span>
          </li>
        {/each}
      </ol>
    </header>

    <div class="onboarding-card-body" class:onboarding-card-body-flush={currentStep === "chat"}>
      {#if currentStep === "welcome"}
        <OnboardingWelcome onnext={() => goTo("profile")} />
      {:else if currentStep === "profile"}
        <OnboardingProfileSelector
          onnext={() => goTo("ai-setup")}
          onback={() => goTo("welcome")}
        />
      {:else if currentStep === "ai-setup"}
        <OnboardingAiSetup
          onnext={() => goTo("chat")}
          onback={() => goTo("profile")}
          onskip={() => goTo("chat")}
          onopencloud={onclose}
        />
      {:else if currentStep === "chat"}
        <OnboardingChatStep
          onback={() => goTo("ai-setup")}
          {onclose}
        />
      {/if}
    </div>

    <footer class="onboarding-card-footer">
      <button
        type="button"
        class="onboarding-skip"
        data-testid="onboarding-skip"
        onclick={handleSkip}
      >
        Configurer plus tard
      </button>
    </footer>
  </div>
</div>

<style>
  .onboarding-overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    align-items: center;
    justify-content: center;
    background: hsl(var(--background) / 0.72);
    backdrop-filter: blur(8px);
    padding: 1.5rem;
  }

  .onboarding-card {
    width: 100%;
    max-width: 720px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    background: hsl(var(--card));
    border: 1px solid hsl(var(--border));
    border-radius: 1rem;
    box-shadow:
      0 24px 64px hsl(var(--foreground) / 0.18),
      0 1px 0 hsl(var(--background) / 0.6) inset;
    overflow: hidden;
  }

  /* AI setup and chat need extra height — give them a fixed minimum so the
     scroll area is comfortable on small viewports. */
  .onboarding-card-tall {
    height: min(86vh, 760px);
  }

  .onboarding-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.875rem 1.25rem;
    border-bottom: 1px solid hsl(var(--border) / 0.7);
    flex-shrink: 0;
  }

  .onboarding-card-title {
    margin: 0;
    font-size: 0.9375rem;
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
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
    transition: color 150ms ease;
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
    font-size: 0.6875rem;
    font-weight: 600;
    transition:
      background 150ms ease,
      color 150ms ease,
      border-color 150ms ease;
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
    color: hsl(var(--primary-foreground, 0 0% 100%));
    border-color: hsl(var(--primary));
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
    font-size: 0.75rem;
    cursor: pointer;
    padding: 0.25rem 0.75rem;
    border-radius: 0.375rem;
    transition:
      color 150ms ease,
      background 150ms ease;
  }

  .onboarding-skip:hover {
    color: hsl(var(--foreground));
    background: hsl(var(--muted));
  }
</style>
