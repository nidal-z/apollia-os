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
  import ChatConversation from "../chat/ChatConversation.svelte";
  import type { ChatSessionDetail, TriggerResult } from "$lib/types";
  import { llmBackends } from "$lib/stores/sse";
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

  const turnIndex = $derived(Math.min(userTurns, TOTAL_TURNS));
  const realReplies = $derived(Math.max(0, userTurns - KICK_MESSAGES));
  const completed = $derived(
    (agentFinalized && realReplies >= MIN_REAL_REPLIES) || turnIndex >= TOTAL_TURNS,
  );
  const llmReady = $derived($llmBackends.length > 0);

  // Show the wrap-up panel either because the agent wrote
  // `onboarding.completed_at` to memory (authoritative signal AFTER the
  // user has actually answered) or because the conversation ran the full
  // 4 turns without a finalize tag (safety net — never leave the user
  // stranded in chat).
  const showWrapUp = $derived(completed && sessionId !== null);

  $effect(() => {
    if (llmReady && !bootstrapStarted && !sessionId && !bootstrapping) {
      bootstrapStarted = true;
      void startChat();
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
    // Best-effort: persist that the user has seen the onboarding to
    // completion so the modal won't reopen on next launch.
    try {
      await invoke("mark_onboarded");
    } catch {
      /* non-blocking */
    }
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

  <div class="chat-body">
    {#if !llmReady}
      <div class="chat-status" data-testid="onboarding-chat-no-llm">
        <AlertCircle size={20} class="text-destructive" aria-hidden="true" />
        <p class="chat-status-title">Aucun moteur LLM disponible</p>
        <p class="chat-status-detail">
          Reviens à l'étape précédente pour configurer un modèle local
          ou un fournisseur cloud — l'agent en a besoin pour dialoguer.
        </p>
        <Button variant="outline" size="sm" onclick={onback}>← Étape précédente</Button>
      </div>
    {:else if bootstrapping}
      <div class="chat-status" data-testid="onboarding-bootstrap">
        <p class="chat-status-detail">Initialisation de l'agent…</p>
      </div>
    {:else if bootstrapError !== null}
      <div class="chat-status chat-status-error">
        <AlertCircle size={20} aria-hidden="true" />
        <p class="chat-status-title">Impossible de démarrer l'onboarding.</p>
        <p class="chat-status-detail">{bootstrapError}</p>
        <Button variant="outline" size="sm" onclick={onback}>← Étape précédente</Button>
      </div>
    {:else if sessionId}
      <ChatConversation
        {sessionId}
        onclose={noop}
        embedded={true}
        hideConfig={true}
      />
    {/if}
  </div>

  {#if showWrapUp}
    <div class="chat-wrapup" data-testid="onboarding-wrapup">
      <div class="chat-wrapup-icon">
        <CheckCircle2 size={18} strokeWidth={2} aria-hidden="true" />
      </div>
      <div class="chat-wrapup-text">
        <p class="chat-wrapup-title">
          {agentFinalized ? "Calibrage terminé" : "Tu peux passer à la suite"}
        </p>
        <p class="chat-wrapup-detail">
          {agentFinalized
            ? "L'agent a enregistré ton profil. Tu peux désormais ouvrir Apollia et explorer."
            : "Si tu as répondu aux questions principales, on peut clore ici."}
        </p>
      </div>
      <Button
        variant="primary-gradient"
        size="default"
        onclick={handleFinish}
        data-testid="onboarding-finish"
      >
        Terminer
      </Button>
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

  .chat-wrapup {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.875rem 1.25rem;
    border-top: 1px solid hsl(var(--border) / 0.7);
    background: hsl(var(--muted) / 0.4);
    flex-shrink: 0;
  }

  .chat-wrapup-icon {
    width: 2rem;
    height: 2rem;
    border-radius: 999px;
    background: hsl(142 71% 35% / 0.12);
    color: hsl(142 71% 35%);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .chat-wrapup-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }

  .chat-wrapup-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .chat-wrapup-detail {
    margin: 0;
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    line-height: 1.4;
  }
</style>
