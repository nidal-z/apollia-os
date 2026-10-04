<script lang="ts">
  import { decideSkip } from "./skipDecision";
  /**
   * Onboarding step 3 - Agent chat, then the profile read over.
   *
   * Spawns an `onboarding-agent` chat session and renders the conversation in
   * the modal. The agent asks a few questions, every one of them optional, and
   * writes nothing: at the end it proposes a profile, which stays in the
   * conversation as the output of its `propose_profile` call. When the
   * conversation is over (`onboarding.completed_at`, stamped by the agent or
   * by the skip button), this step reads that proposal and:
   *
   *   - on first launch, shows it in {@link OnboardingProfileReview}, where
   *     "Save" writes the default profile and "Skip without saving" writes
   *     nothing;
   *   - when Settings > Profile asked for a profile to be completed, hands it
   *     back to that page, which shows it in its own form for the same review.
   *
   * Safety net: if no LLM backend is registered when this step mounts
   * (e.g. the user skipped AI Setup), we surface the situation explicitly
   * instead of letting the first message silently fail.
   */
  import { onMount } from "svelte";
  import { locale, t } from "svelte-i18n";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import OnboardingProfileReview from "./OnboardingProfileReview.svelte";
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
  import {
    extractProposal,
    knownFieldsLine,
    type EnrichmentRequest,
    type ProfileProposal,
  } from "$lib/onboarding/profileProposal";
  import { isChatComplete, runDirectSkip } from "$lib/onboarding/skipFlow";
  import {
    llmBackends,
    llmBackendsHydrated,
    refreshLlmBackends,
  } from "$lib/stores/sse";
  import { pendingProfileProposal, profileEnrichment } from "$lib/stores/onboarding";
  import { Button } from "$lib/components/ui/button";
  import { AlertCircle, Check, Loader2 } from "lucide-svelte";
  import { fieldLabel } from "../settings/profile/profileForm";

  interface Props {
    onback: () => void;
    /** Called once the profile was saved or skipped. */
    onclose: () => void;
    /**
     * Incremented by the orchestrator's footer to skip the questions: each
     * increment ends the conversation directly through the backend, with no
     * model turn (conversational nudge as fallback only), and opens the
     * review on whatever was proposed so far.
     */
    skipSignal?: number;
    /**
     * Called when the step is asked to skip and cannot: the conversation never
     * started, so there is nothing to finalize. The parent abandons onboarding
     * rather than leaving the user on a card whose only button does nothing.
     */
    onstranded?: () => void;
  }

  const { onback, onclose, skipSignal = 0, onstranded }: Props = $props();

  let sessionId = $state<string | null>(null);
  let bootstrapping = $state(false);
  let bootstrapError = $state<string | null>(null);
  let userTurns = $state(0);
  let agentFinalized = $state(false);
  let proposal = $state<ProfileProposal | null>(null);
  // The profile Settings > Profile asked to complete; `null` on first launch.
  let enrichment = $state<EnrichmentRequest | null>(null);
  // Set by "Back" on the review: the conversation is shown again until the
  // person asks for the proposal once more.
  let reviewClosed = $state(false);
  // Set once the proposal was read after the end of the conversation, so the
  // form never opens on a proposal that is still on its way.
  let proposalRead = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let bootstrapStarted = false;

  // Visual progress bar: the agent asks at most four questions. It's a UI
  // hint, NOT a completion criterion.
  const TOTAL_TURNS = 5;
  // The first user message is the auto-kick greeting injected by this
  // component to prime the agent. We don't count it when judging whether
  // the user has actually engaged.
  const KICK_MESSAGES = 1;
  // Minimum *real* user replies (excluding the kick) required before we
  // trust the agent's `completed_at` signal: a proposal cannot come before
  // the person said anything.
  const MIN_REAL_REPLIES = 1;
  // Safety net: how many real replies the user types before the review opens
  // *without* the `onboarding.completed_at` signal. Deliberately generous:
  // the real exits are the agent's proposal and the two explicit buttons.
  const SAFETY_REPLIES = 12;

  const turnIndex = $derived(Math.min(userTurns, TOTAL_TURNS));
  const realReplies = $derived(Math.max(0, userTurns - KICK_MESSAGES));
  // Set when the person explicitly skipped the questions: an explicit action,
  // so it bypasses the MIN_REAL_REPLIES guard.
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

  let finishing = $state(false);
  const canFinishEarly = $derived(
    sessionId !== null && !bootstrapping && realReplies >= MIN_REAL_REPLIES && !agentFinalized,
  );

  async function finishEarly(): Promise<void> {
    if (!sessionId || finishing) return;
    finishing = true;
    try {
      // Nudge the agent to propose now, from what was already said.
      await sendChatMessage(sessionId, $t("onboarding_chat.finish_early_message"));
    } catch {
      // Non-fatal: the agent may still propose on a later turn.
    } finally {
      finishing = false;
    }
  }

  const showReview = $derived(
    completed && sessionId !== null && proposalRead && !reviewClosed,
  );

  $effect(() => {
    if (llmReady && !bootstrapStarted && !sessionId && !bootstrapping) {
      bootstrapStarted = true;
      void startChat();
    }
  });

  // The moment the conversation is over, read the proposal it holds. A
  // conversation asked for by Settings > Profile hands it back to that page
  // and closes; nothing is written here either way.
  let handedOver = false;
  $effect(() => {
    if (!completed || !sessionId || handedOver) return;
    handedOver = true;
    void readProposal().then(() => {
      proposalRead = true;
      if (!enrichment) return;
      pendingProfileProposal.set({ profileId: enrichment.profileId, proposal: proposal ?? {} });
      onclose();
    });
  });

  // "Skip the questions", requested from the orchestrator footer. Finalize
  // directly through the backend command: no model turn, the completion key
  // is stamped and the review opens immediately. On failure, fall back to the
  // conversational nudge so the user is never stranded.
  let lastSkipSignal = 0;
  $effect(() => {
    const signal = skipSignal;
    if (signal > lastSkipSignal) {
      lastSkipSignal = signal;
      if (reviewClosed) {
        // Back on the conversation after a first look: show the review again.
        void reopenReview();
        return;
      }
      switch (decideSkip({ sessionId, completed })) {
        case "finalize":
          void skipDirectly();
          break;
        case "abandon":
          onstranded?.();
          break;
        case "ignore":
          break;
      }
    }
  });

  async function skipDirectly(): Promise<void> {
    const path = await runDirectSkip(finalizeOnboardingChat, finishEarly);
    if (path === "finalized") {
      agentFinalized = true;
      skippedDirectly = true;
      stopPolling();
    }
  }

  /** Show the review again, on the latest proposal of the conversation. */
  async function reopenReview(): Promise<void> {
    await readProposal();
    reviewClosed = false;
  }

  function stopPolling(): void {
    if (pollTimer !== undefined) {
      clearInterval(pollTimer);
      pollTimer = undefined;
    }
  }

  /** Read the latest proposal the conversation holds, if any. */
  async function readProposal(): Promise<void> {
    if (!sessionId) return;
    try {
      const detail = await getChatSession(sessionId);
      userTurns = detail.messages.filter((m) => m.role === "user").length;
      proposal = extractProposal(detail.messages) ?? proposal;
    } catch {
      // Non-critical: the review opens on what was read so far.
    }
  }

  async function pollSession(): Promise<void> {
    await readProposal();

    // The conversation is over once `onboarding.completed_at` is in the
    // agent's memory, which its proposal writes last.
    if (!agentFinalized) {
      try {
        if (await checkOnboardingFinalized()) {
          agentFinalized = true;
          stopPolling();
        }
      } catch {
        // Non-critical - try again next tick.
      }
    }
  }

  /** The first message: a greeting, plus what the profile to complete already holds. */
  function kickMessage(request: EnrichmentRequest | null): string {
    const known = request
      ? knownFieldsLine(request.fields, (field) => fieldLabel(field, $locale))
      : "";
    return known
      ? $t("onboarding_chat.kick_message_resume", { values: { known } })
      : $t("onboarding_chat.kick_message");
  }

  async function startChat(): Promise<void> {
    bootstrapping = true;
    bootstrapError = null;
    try {
      // A request from Settings > Profile completes an existing profile; a
      // normal launch is a fresh run. Neither touches a profile.
      enrichment = get(profileEnrichment);
      const result = enrichment ? await resumeOnboarding() : await triggerOnboarding();
      profileEnrichment.set(null);
      sessionId = result.session_id;
      bootstrapping = false;

      // Kick the agent so it produces its opening turn without waiting
      // for the user to type first. Failures here are non-fatal - the
      // agent will still respond once the user sends a real message.
      //
      // Translated, and not for cosmetics: this is the first message of the
      // conversation, so it is what the model reads to decide which language
      // to answer in. Hardcoded in French, it made the agent greet an English
      // user in French and keep going in French for the whole conversation,
      // whatever language they had just picked two screens earlier.
      try {
        await sendChatMessage(result.session_id, kickMessage(enrichment));
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
    return stopPolling;
  });
</script>

<div class="chat-step" data-testid="onboarding-chat-step">
  <div class="chat-progress" aria-label={$t("onboarding_chat.progress_label")}>
    {#each Array(TOTAL_TURNS) as _, i}
      <span
        class="chat-progress-pip"
        class:active={i < turnIndex || completed}
        aria-current={i === turnIndex && !completed ? "step" : undefined}
      ></span>
    {/each}
    <span class="chat-progress-check" class:active={completed}>
      <Check size={12} aria-hidden="true" />
    </span>
  </div>

  {#if showReview && !enrichment}
    <!-- The proposal replaces the conversation: the person reads it over in
         the profile form, and nothing is written before "Save". -->
    <div class="chat-body chat-body-stage">
      <OnboardingProfileReview
        {proposal}
        ondone={onclose}
        onback={() => (reviewClosed = true)}
      />
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
          {$t("onboarding_chat.review_banner")}
        </p>
        <ChatConversation
          {sessionId}
          onclose={noop}
          embedded={true}
          hideConfig={true}
        />
        {#if reviewClosed}
          <div class="chat-finish-bar">
            <span class="chat-finish-hint">{$t("onboarding_chat.review_again_hint")}</span>
            <Button
              variant="outline"
              size="sm"
              onclick={reopenReview}
              data-testid="onboarding-review-open"
            >
              {$t("onboarding_chat.review_again")}
            </Button>
          </div>
        {:else if canFinishEarly}
          <div class="chat-finish-bar">
            <span class="chat-finish-hint">{$t("onboarding_chat.finish_early_hint")}</span>
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
    transition: background var(--motion-base) ease;
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
    font-size: var(--text-caption);
    font-weight: 700;
    transition:
      background var(--motion-base) ease,
      color var(--motion-base) ease,
      border-color var(--motion-base) ease;
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

  /* When the chat body hosts the profile review instead of the
     conversation: no chat history, no banner, the form owns the scroll. */
  .chat-body-stage {
    align-items: stretch;
    background: hsl(var(--card));
  }

  .chat-banner {
    margin: 0;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid hsl(var(--border) / 0.5);
    background: hsl(var(--primary) / 0.06);
    color: hsl(var(--muted-foreground));
    font-size: var(--text-body-xs);
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
    font-size: var(--text-body-xs);
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
    font-size: var(--text-heading-sm);
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .chat-status-detail {
    margin: 0;
    max-width: 28rem;
    font-size: var(--text-body-sm);
    line-height: 1.5;
  }

  .chat-status-error {
    color: hsl(var(--destructive));
  }
  .chat-status-error .chat-status-detail {
    color: hsl(var(--muted-foreground));
  }
</style>
