<script lang="ts">
  /**
   * Onboarding step 4 - Agent chat.
   *
   * Final step of the onboarding flow. Spawns an `onboarding-agent` chat
   * session and renders the conversation in the modal. The agent first
   * collects the four mandatory facts (`user.name`, `user.role`,
   * `user.agents.hitl`, `user.constraints.sovereignty`), then runs an optional
   * profile enrichment (Tier 2). It writes `onboarding.completed_at` at
   * closure, which the supervisor turns into the `OnboardingCompleted` runtime
   * event (handled by the parent modal). The user can end early via the
   * explicit "finish" button, which nudges the agent to close.
   *
   * Safety net: if no LLM backend is registered when this step mounts
   * (e.g. the user skipped AI Setup), we surface the situation explicitly
   * instead of letting the first message silently fail.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import OnboardingPermissionStep from "./OnboardingPermissionStep.svelte";
  import OnboardingConfetti from "./OnboardingConfetti.svelte";
  import {
    checkOnboardingFinalized,
    finalizeOnboardingChat,
    getChatSession,
    resumeOnboarding,
    sendChatMessage,
    triggerOnboarding,
  } from "$lib/ipc/onboarding";
  import { get } from "svelte/store";
  import { deriveLlmState } from "$lib/onboarding/llmState";
  import { isChatComplete, runDirectSkip } from "$lib/onboarding/skipFlow";
  import {
    llmBackends,
    llmBackendsHydrated,
    refreshLlmBackends,
  } from "$lib/stores/sse";
  import { onboardingResumeMode } from "$lib/stores/onboarding";
  import { Button } from "$lib/components/ui/button";
  import { AlertCircle, CheckCircle2, Loader2 } from "lucide-svelte";

  interface Props {
    onback: () => void;
    /** Called when the user clicks "Terminer" or auto-close fires. */
    onclose: () => void;
    /**
     * Incremented by the orchestrator's footer to request an early wrap-up:
     * skip the remaining OPTIONAL questions and route forward to the
     * permissions phase. It must never abandon onboarding (that would skip
     * permissions). Each increment finalizes the chat directly through the
     * backend, with no model turn (conversational nudge as fallback only).
     */
    skipSignal?: number;
  }

  const { onback, onclose, skipSignal = 0 }: Props = $props();

  let sessionId = $state<string | null>(null);
  let bootstrapping = $state(false);
  let bootstrapError = $state<string | null>(null);
  let userTurns = $state(0);
  let agentFinalized = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let bootstrapStarted = false;

  // Visual progress bar. The flow now has two phases: a mandatory 3-question
  // calibration, then an optional profile enrichment (~a handful of turns).
  // It's a UI hint, NOT a completion criterion.
  const TOTAL_TURNS = 8;
  // The first user message is the auto-kick greeting injected by this
  // component to prime the agent. We don't count it when judging whether
  // the user has actually engaged.
  const KICK_MESSAGES = 1;
  // Minimum *real* user replies (excluding the kick) required before we
  // trust the agent's `completed_at` signal - guards against stale memory
  // state from a previous broken session triggering the wrap-up panel
  // before the conversation has even started.
  const MIN_REAL_REPLIES = 2;
  // Number of real replies after which calibration (name/role/hitl/
  // sovereignty) is complete and the conversation has moved into the
  // optional Tier 2 enrichment phase.
  const CALIBRATION_REPLIES = 3;
  // Safety net: how many real replies the user types before we let the
  // wrap-up panel appear *without* the agent's authoritative
  // `onboarding.completed_at` signal. Deliberately generous now that the
  // conversation runs through the optional enrichment: the real exits are
  // the agent's [PROFILE] closure and the explicit "finish" button, so this
  // backstop should almost never fire before either.
  const SAFETY_REPLIES = 12;

  const turnIndex = $derived(Math.min(userTurns, TOTAL_TURNS));
  const realReplies = $derived(Math.max(0, userTurns - KICK_MESSAGES));
  // Set when the operator explicitly skipped the optional questions: an
  // explicit user action, so it bypasses the MIN_REAL_REPLIES stale-memory
  // guard (which only exists to distrust a leftover `completed_at`).
  let skippedDirectly = $state(false);
  const completed = $derived(
    isChatComplete(
      { skippedDirectly, agentFinalized, realReplies },
      MIN_REAL_REPLIES,
      SAFETY_REPLIES,
    ),
  );
  // Tri-state: "checking" until the backend list has been hydrated once, so
  // an engine still registering (store-hydration race after AI setup) shows a
  // neutral starting status instead of a destructive "no engine" card.
  const llmState = $derived(deriveLlmState($llmBackendsHydrated, $llmBackends.length));
  const llmReady = $derived(llmState === "ready");

  // Phase caption + explicit early-finish. Calibration first, then optional
  // enrichment. The user can end at any time once calibration is under way;
  // the agent will still refuse to close before name + role are collected.
  const enrichmentPhase = $derived(realReplies >= CALIBRATION_REPLIES);
  let finishing = $state(false);
  const canFinishEarly = $derived(
    sessionId !== null && !completed && !bootstrapping && realReplies >= MIN_REAL_REPLIES,
  );

  async function finishEarly(): Promise<void> {
    if (!sessionId || finishing) return;
    finishing = true;
    try {
      // Nudge the agent to wrap up. It closes with [PROFILE] once the
      // mandatory keys are present, which writes `onboarding.completed_at`.
      await sendChatMessage(sessionId, $t("onboarding_chat.finish_early_message"));
    } catch {
      // Non-fatal: the agent may still close on a later turn.
    } finally {
      finishing = false;
    }
  }

  // Permissions step: between agent finalisation and the wrap-up, the agent
  // may have persisted a list of permission rule proposals under
  // `onboarding.proposed_rules`. The OnboardingPermissionStep component
  // surfaces them as inline approval cards. This flag is true while there
  // is at least one proposal still pending.
  let permissionsPending = $state(false);

  // Show the wrap-up panel either because the agent wrote
  // `onboarding.completed_at` to memory (authoritative signal AFTER the
  // user has actually answered) or because the conversation ran the full
  // 4 turns without a finalize tag (safety net - never leave the user
  // stranded in chat).
  // Wrap-up is suppressed while permission cards are still pending so the
  // user is forced to triage them before clicking "Terminer".
  const showPermissions = $derived(
    completed && sessionId !== null && permissionsPending,
  );
  const showWrapUp = $derived(
    completed && sessionId !== null && !showPermissions,
  );

  $effect(() => {
    if (llmReady && !bootstrapStarted && !sessionId && !bootstrapping) {
      bootstrapStarted = true;
      void startChat();
    }
  });

  // The moment the flow completes, enter the permissions sub-step. Keyed on
  // `completed` (not just `agentFinalized`) so the permissions phase ALWAYS
  // runs before the wrap-up, including the safety-net completion path. The
  // OnboardingPermissionStep component loads the proposals list and calls
  // oncomplete() immediately if it's empty (no cards to show), which unblocks
  // the wrap-up. This is what guarantees permissions are never skipped.
  let permissionsEntered = false;
  $effect(() => {
    if (completed && !permissionsEntered) {
      permissionsEntered = true;
      permissionsPending = true;
    }
  });

  // Forward wrap-up requested from the orchestrator footer ("skip the optional
  // questions"). Finalize directly through the backend command: no model turn,
  // the completion key is stamped and the flow routes into the permissions
  // phase immediately. Never dismisses onboarding. On failure, fall back to
  // the conversational nudge so the user is never stranded.
  let lastSkipSignal = 0;
  $effect(() => {
    const signal = skipSignal;
    if (signal > lastSkipSignal) {
      lastSkipSignal = signal;
      if (sessionId && !completed) void skipDirectly();
    }
  });

  async function skipDirectly(): Promise<void> {
    const path = await runDirectSkip(finalizeOnboardingChat, finishEarly);
    if (path === "finalized") {
      agentFinalized = true;
      skippedDirectly = true;
      if (pollTimer !== undefined) {
        clearInterval(pollTimer);
        pollTimer = undefined;
      }
    }
  }

  async function pollSession(): Promise<void> {
    if (!sessionId) return;
    try {
      const detail = await getChatSession(sessionId);
      userTurns = detail.messages.filter((m) => m.role === "user").length;
    } catch {
      // Non-critical - retry on next tick.
    }

    // Authoritative completion signal: the agent has written
    // `onboarding.completed_at` to its semantic memory. Once true, we stop
    // polling and switch to the wrap-up panel.
    if (!agentFinalized) {
      try {
        const done = await checkOnboardingFinalized();
        if (done) {
          agentFinalized = true;
          if (pollTimer !== undefined) {
            clearInterval(pollTimer);
            pollTimer = undefined;
          }
        }
      } catch {
        // Non-critical - try again next tick.
      }
    }
  }

  async function handleFinish(): Promise<void> {
    // The modal hands back to a fully usable application. Discovery continues
    // from the Getting started band on the dashboard, which the user opens when
    // they choose to: nothing launches on its own after this point.
    onclose();
  }

  async function startChat(): Promise<void> {
    bootstrapping = true;
    bootstrapError = null;
    try {
      // Resume mode (from the "complete your profile" entry point) keeps the
      // already-collected profile; a normal launch resets for a fresh run.
      const resume = get(onboardingResumeMode);
      const result = resume ? await resumeOnboarding() : await triggerOnboarding();
      onboardingResumeMode.set(false);
      sessionId = result.session_id;
      bootstrapping = false;

      // Kick the agent so it produces its opening turn without waiting
      // for the user to type first. Failures here are non-fatal - the
      // agent will still respond once the user sends a real message.
      //
      // Translated, and not for cosmetics: this is the first message of the
      // conversation, so it is what the model reads to decide which language
      // to answer in. Hardcoded in French, it made the agent greet an English
      // user in French and keep going in French for the whole calibration,
      // whatever language they had just picked two screens earlier.
      try {
        await sendChatMessage(result.session_id, $t("onboarding_chat.kick_message"));
      } catch {
        /* ignore */
      }

      pollTimer = setInterval(pollSession, 3000);
    } catch (err) {
      bootstrapping = false;
      bootstrapError = err instanceof Error ? err.message : String(err);
      bootstrapStarted = false;
    }
  }

  function noop(): void {
    // ChatConversation requires onclose; the orchestrator owns close lifecycle.
  }

  onMount(() => {
    // The AI-setup step registers the backend without an SSE push reaching
    // this store (component-local success flag only); refresh eagerly instead
    // of waiting for the 10 s watchdog.
    void refreshLlmBackends();
    return () => {
      if (pollTimer !== undefined) clearInterval(pollTimer);
    };
  });
</script>

<div class="chat-step" data-testid="onboarding-chat-step">
  <div class="chat-progress" aria-label={$t("onboarding_chat.progress_label")}>
    {#each Array(TOTAL_TURNS) as _, i}
      <span
        class="chat-progress-pip"
        class:active={i < turnIndex}
        aria-current={i === turnIndex && !completed ? "step" : undefined}
      ></span>
    {/each}
    <span class="chat-progress-check" class:active={completed}>✓</span>
  </div>
  {#if sessionId && !completed}
    <p class="chat-phase" data-testid="onboarding-chat-phase">
      {enrichmentPhase
        ? $t("onboarding_chat.phase_enrichment")
        : $t("onboarding_chat.phase_calibration")}
    </p>
  {/if}

  {#if showPermissions}
    <!-- Permission cards take over the modal once the agent finalises. We
         intentionally hide the chat body to avoid any visual overlap with
         the cards or the (translucent) wrap-up panel below. -->
    <div class="chat-body chat-body-stage">
      <OnboardingPermissionStep
        oncomplete={() => (permissionsPending = false)}
      />
    </div>
  {:else if showWrapUp}
    <!-- Final celebration screen. Replaces the chat entirely so the user
         sees a clean confirmation without the previous Q&A bleeding through. -->
    <div class="chat-body chat-body-stage">
      <div class="celebration" data-testid="onboarding-celebration">
        {#if agentFinalized}
          <OnboardingConfetti />
        {/if}
        <div class="celebration-icon">
          <CheckCircle2 size={32} strokeWidth={2} aria-hidden="true" />
        </div>
        <h3 class="celebration-title">
          {agentFinalized
            ? $t("onboarding_chat.wrapup_title_finalized")
            : $t("onboarding_chat.wrapup_title_safety")}
        </h3>
        <p class="celebration-detail">
          {agentFinalized
            ? $t("onboarding_chat.wrapup_detail_finalized")
            : $t("onboarding_chat.wrapup_detail_safety")}
        </p>
        <Button
          variant="primary-gradient"
          size="lg"
          onclick={handleFinish}
          data-testid="onboarding-finish"
        >
          {$t("onboarding_chat.finish")}
        </Button>
      </div>
    </div>
  {:else}
    <div class="chat-body">
      {#if llmState === "checking"}
        <div class="chat-status" data-testid="onboarding-chat-llm-starting">
          <Loader2 size={20} class="animate-spin" aria-hidden="true" />
          <p class="chat-status-title">{$t("onboarding_chat.llm_starting_title")}</p>
          <p class="chat-status-detail">{$t("onboarding_chat.llm_starting_detail")}</p>
        </div>
      {:else if llmState === "none"}
        <div class="chat-status" data-testid="onboarding-chat-no-llm">
          <AlertCircle size={20} class="text-destructive" aria-hidden="true" />
          <p class="chat-status-title">{$t("onboarding_chat.no_llm_title")}</p>
          <p class="chat-status-detail">{$t("onboarding_chat.no_llm_detail")}</p>
          <Button variant="outline" size="sm" onclick={onback}>
            {$t("onboarding_chat.back_step")}
          </Button>
        </div>
      {:else if bootstrapping}
        <div class="chat-status" data-testid="onboarding-bootstrap">
          <p class="chat-status-detail">{$t("onboarding_chat.bootstrap")}</p>
        </div>
      {:else if bootstrapError !== null}
        <div class="chat-status chat-status-error">
          <AlertCircle size={20} aria-hidden="true" />
          <p class="chat-status-title">{$t("onboarding_chat.bootstrap_failed")}</p>
          <p class="chat-status-detail">{bootstrapError}</p>
          <Button variant="outline" size="sm" onclick={onback}>
            {$t("onboarding_chat.back_step")}
          </Button>
        </div>
      {:else if sessionId}
        <p class="chat-banner" data-testid="onboarding-chat-banner">
          {$t("onboarding_chat.permission_banner")}
        </p>
        <ChatConversation
          {sessionId}
          onclose={noop}
          embedded={true}
          hideConfig={true}
        />
        {#if canFinishEarly}
          <div class="chat-finish-bar">
            <span class="chat-finish-hint">
              {enrichmentPhase
                ? $t("onboarding_chat.finish_early_hint_enrichment")
                : $t("onboarding_chat.finish_early_hint_calibration")}
            </span>
            <Button
              variant="outline"
              size="sm"
              onclick={finishEarly}
              disabled={finishing}
              data-testid="onboarding-finish-early"
            >
              {$t("onboarding_chat.finish_early")}
            </Button>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .chat-step {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .chat-progress {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    padding: 0.625rem 1rem;
    border-bottom: 1px solid hsl(var(--border) / 0.6);
    background: hsl(var(--muted) / 0.3);
  }

  .chat-progress-pip {
    width: 1.5rem;
    height: 0.25rem;
    border-radius: 99px;
    background: hsl(var(--border));
    transition: background 200ms ease;
  }

  .chat-progress-pip.active {
    background: linear-gradient(90deg, hsl(var(--primary)), hsl(var(--secondary)));
  }

  .chat-progress-check {
    margin-left: 0.25rem;
    width: 1.25rem;
    height: 1.25rem;
    border-radius: 999px;
    border: 1px solid hsl(var(--border));
    background: transparent;
    color: hsl(var(--muted-foreground));
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.6875rem;
    font-weight: 700;
    transition:
      background 200ms ease,
      color 200ms ease,
      border-color 200ms ease;
  }

  .chat-progress-check.active {
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    border-color: hsl(var(--primary));
  }

  .chat-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* When the chat body hosts the permission cards or the celebration
     screen instead of the conversation, centre the content vertically
     and give it room to breathe - no chat history, no banner, no
     overflow blending. */
  .chat-body-stage {
    align-items: stretch;
    justify-content: center;
    overflow-y: auto;
    background: hsl(var(--card));
  }

  .celebration {
    position: relative;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    padding: 2.5rem 1.75rem;
    text-align: center;
  }
  .celebration-icon {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 999px;
    background: hsl(var(--success) / 0.15);
    color: hsl(var(--success));
    display: flex;
    align-items: center;
    justify-content: center;
    animation: celebration-pop var(--motion-slow) var(--ease-spring) both;
  }
  @keyframes celebration-pop {
    from {
      opacity: 0;
      transform: scale(0.5);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .celebration-icon {
      animation: none;
    }
  }
  .celebration-title {
    margin: 0;
    font-size: 1.125rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }
  .celebration-detail {
    margin: 0;
    max-width: 32rem;
    font-size: 0.875rem;
    line-height: 1.5;
    color: hsl(var(--muted-foreground));
  }

  .chat-phase {
    margin: 0;
    padding: 0.3rem 1rem 0;
    text-align: center;
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: hsl(var(--muted-foreground));
  }

  .chat-banner {
    margin: 0;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid hsl(var(--border) / 0.5);
    background: hsl(var(--primary) / 0.06);
    color: hsl(var(--muted-foreground));
    font-size: 0.75rem;
    line-height: 1.4;
    text-align: center;
    flex-shrink: 0;
  }

  .chat-finish-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.5rem 1rem;
    border-top: 1px solid hsl(var(--border) / 0.6);
    background: hsl(var(--muted) / 0.3);
    flex-shrink: 0;
  }

  .chat-finish-hint {
    font-size: 0.75rem;
    line-height: 1.3;
    color: hsl(var(--muted-foreground));
  }

  .chat-status {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 2rem;
    text-align: center;
    color: hsl(var(--muted-foreground));
  }

  .chat-status-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .chat-status-detail {
    margin: 0;
    max-width: 28rem;
    font-size: 0.8125rem;
    line-height: 1.5;
  }

  .chat-status-error {
    color: hsl(var(--destructive));
  }
  .chat-status-error .chat-status-detail {
    color: hsl(var(--muted-foreground));
  }

  /* (Legacy `.chat-wrapup` styles removed - the celebration screen now
     replaces the chat body entirely instead of stacking under it.) */
</style>
