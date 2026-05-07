<script lang="ts">
  /**
   * Onboarding step 4 — Agent chat.
   *
   * Final step of the onboarding flow. Spawns an `onboarding-agent` chat
   * session and renders the conversation in the modal. The agent collects
   * the four mandatory facts (`user.name`, `user.role`, `user.agents.hitl`,
   * `user.constraints.sovereignty`) over ~4 turns, then writes
   * `onboarding.completed_at` which the supervisor turns into the
   * `OnboardingCompleted` runtime event (handled by the parent modal).
   *
   * Safety net: if no LLM backend is registered when this step mounts
   * (e.g. the user skipped AI Setup), we surface the situation explicitly
   * instead of letting the first message silently fail.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import OnboardingPermissionStep from "./OnboardingPermissionStep.svelte";
  import type { ChatSessionDetail, TriggerResult } from "$lib/types";
  import { llmBackends } from "$lib/stores/sse";
  import { onboardingTourActive } from "$lib/stores/tour";
  import { Button } from "$lib/components/ui/button";
  import { AlertCircle, CheckCircle2 } from "lucide-svelte";

  interface Props {
    onback: () => void;
    /** Called when the user clicks "Terminer" or auto-close fires. */
    onclose: () => void;
  }

  const { onback, onclose }: Props = $props();

  let sessionId = $state<string | null>(null);
  let bootstrapping = $state(false);
  let bootstrapError = $state<string | null>(null);
  let userTurns = $state(0);
  let agentFinalized = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let bootstrapStarted = false;

  // Visual progress bar : the wizard advertises "4 tours" (Q1, Q2, Q3,
  // closure). It's a UI hint, NOT a completion criterion.
  const TOTAL_TURNS = 4;
  // The first user message is the auto-kick "Bonjour !" injected by this
  // component to prime the agent. We don't count it when judging whether
  // the user has actually engaged.
  const KICK_MESSAGES = 1;
  // Minimum *real* user replies (excluding the kick) required before we
  // trust the agent's `completed_at` signal — guards against stale memory
  // state from a previous broken session triggering the wrap-up panel
  // before the conversation has even started.
  const MIN_REAL_REPLIES = 2;
  // Safety net: how many real replies the user has to type before we let
  // the wrap-up panel appear *without* the agent's authoritative
  // `onboarding.completed_at` signal. The 3 structured questions need 3
  // real replies; we wait for one extra (4) so the agent has had a chance
  // to either finalize or retry a missed answer. Without this slack,
  // hitting `TOTAL_TURNS` (which counts the kick) would surface the
  // wrap-up the very moment the user answered Q3, even if the agent is
  // still processing their reply.
  const SAFETY_REPLIES = 4;

  const turnIndex = $derived(Math.min(userTurns, TOTAL_TURNS));
  const realReplies = $derived(Math.max(0, userTurns - KICK_MESSAGES));
  const completed = $derived(
    (agentFinalized && realReplies >= MIN_REAL_REPLIES) || realReplies >= SAFETY_REPLIES,
  );
  const llmReady = $derived($llmBackends.length > 0);

  // Permissions step: between agent finalisation and the wrap-up, the agent
  // may have persisted a list of permission rule proposals under
  // `onboarding.proposed_rules`. The OnboardingPermissionStep component
  // surfaces them as inline approval cards. This flag is true while there
  // is at least one proposal still pending.
  let permissionsPending = $state(false);

  // Show the wrap-up panel either because the agent wrote
  // `onboarding.completed_at` to memory (authoritative signal AFTER the
  // user has actually answered) or because the conversation ran the full
  // 4 turns without a finalize tag (safety net — never leave the user
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

  // The moment the agent finalises, enter the permissions sub-step. The
  // OnboardingPermissionStep component will load the proposals list and
  // call oncomplete() if it's empty (no cards to show), unblocking wrap-up.
  let permissionsEntered = false;
  $effect(() => {
    if (agentFinalized && !permissionsEntered) {
      permissionsEntered = true;
      permissionsPending = true;
    }
  });

  async function pollSession(): Promise<void> {
    if (!sessionId) return;
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", {
        sessionId,
      });
      userTurns = detail.messages.filter((m) => m.role === "user").length;
    } catch {
      // Non-critical — retry on next tick.
    }

    // Authoritative completion signal: the agent has written
    // `onboarding.completed_at` to its semantic memory. Once true, we stop
    // polling and switch to the wrap-up panel.
    if (!agentFinalized) {
      try {
        const done = await invoke<boolean>("check_onboarding_finalized");
        if (done) {
          agentFinalized = true;
          if (pollTimer !== undefined) {
            clearInterval(pollTimer);
            pollTimer = undefined;
          }
        }
      } catch {
        // Non-critical — try again next tick.
      }
    }
  }

  async function handleFinish(): Promise<void> {
    // Hand-off to the post-onboarding guided tour: close the modal and let
    // the App-level `OnboardingTourRunner` overlay take over. The runner
    // is responsible for calling `mark_onboarded` once it terminates so
    // the modal only re-opens at next launch if the tour itself was
    // interrupted (and the backend phase isn't `done`).
    onboardingTourActive.set(true);
    onclose();
  }

  async function startChat(): Promise<void> {
    bootstrapping = true;
    bootstrapError = null;
    try {
      const result = await invoke<TriggerResult>("trigger_onboarding", {
        topic: null,
        profile: null,
      });
      sessionId = result.session_id;
      bootstrapping = false;

      // Kick the agent so it produces its opening turn without waiting
      // for the user to type first. Failures here are non-fatal — the
      // agent will still respond once the user sends a real message.
      try {
        await invoke("send_chat_message", {
          sessionId: result.session_id,
          content: "Bonjour !",
        });
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
    return () => {
      if (pollTimer !== undefined) clearInterval(pollTimer);
    };
  });
</script>

<div class="chat-step" data-testid="onboarding-chat-step">
  <div class="chat-progress" aria-label="Tours de l'onboarding">
    {#each Array(TOTAL_TURNS) as _, i}
      <span
        class="chat-progress-pip"
        class:active={i < turnIndex}
        aria-current={i === turnIndex && !completed ? "step" : undefined}
      ></span>
    {/each}
    <span class="chat-progress-check" class:active={completed}>✓</span>
  </div>

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
      {#if !llmReady}
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
    color: hsl(var(--primary-foreground, 0 0% 100%));
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
     and give it room to breathe — no chat history, no banner, no
     overflow blending. */
  .chat-body-stage {
    align-items: stretch;
    justify-content: center;
    overflow-y: auto;
    background: hsl(var(--card));
  }

  .celebration {
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
    background: hsl(142 71% 35% / 0.15);
    color: hsl(142 71% 35%);
    display: flex;
    align-items: center;
    justify-content: center;
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

  /* (Legacy `.chat-wrapup` styles removed — the celebration screen now
     replaces the chat body entirely instead of stacking under it.) */
</style>
