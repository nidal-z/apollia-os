<script lang="ts">
  /**
   * Agent-driven onboarding modal.
   *
   * Replaces the legacy 7-phase machine. The onboarding-agent v2.1.0
   * (agents/system/onboarding-agent) drives the entire flow as a chat
   * conversation. The frontend's only jobs are:
   *
   *   - display the chat in a dedicated overlay,
   *   - close itself when the agent completes (RuntimeEvent::OnboardingCompleted),
   *   - offer a "Skip" escape hatch (dismiss_onboarding).
   *
   * Visibility is driven by the supervisor: at first launch, when
   * UserMemory is empty, RuntimeEvent::OnboardingRequired is emitted on
   * the "runtime-event" Tauri channel with `category: "onboarding-changed"`.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import type { ChatSessionDetail, TriggerResult } from "$lib/types";

  interface Props {
    onclose: () => void;
  }

  const { onclose }: Props = $props();

  const TOTAL_TURNS = 4;

  let sessionId = $state<string | null>(null);
  let bootstrapping = $state(true);
  let bootstrapError = $state<string | null>(null);
  let userTurns = $state(0);
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let unlistenRuntime: UnlistenFn | null = null;
  let rootEl = $state<HTMLDivElement | null>(null);

  const turnIndex = $derived(Math.min(userTurns, TOTAL_TURNS));
  const completed = $derived(turnIndex >= TOTAL_TURNS);

  function noop(): void {
    // ChatConversation requires onclose; the modal owns close lifecycle.
  }

  function captureEscape(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
    }
  }

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
  }

  async function handleSkip(): Promise<void> {
    try {
      await invoke("dismiss_onboarding");
    } catch {
      // best-effort; closing anyway
    }
    onclose();
  }

  onMount(() => {
    // Focus trap entry point: bring focus to the overlay so subsequent
    // tab navigation stays inside it. ChatInput auto-focuses its textarea
    // once mounted.
    rootEl?.focus();

    void (async () => {
      try {
        const result = await invoke<TriggerResult>("trigger_onboarding", {
          topic: null,
          profile: null,
        });
        sessionId = result.session_id;
        bootstrapping = false;

        // Kick the agent so it produces its opening turn without waiting
        // for the user to type first.
        try {
          await invoke("send_chat_message", {
            sessionId: result.session_id,
            content: "Bonjour !",
          });
        } catch {
          // The agent will still respond once the user types.
        }

        pollTimer = setInterval(pollSession, 3000);
      } catch (err) {
        bootstrapping = false;
        bootstrapError = err instanceof Error ? err.message : String(err);
      }
    })();

    // Listen to OnboardingCompleted to auto-close. The supervisor emits
    // it when the agent writes onboarding.completed_at.
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
      if (pollTimer !== undefined) clearInterval(pollTimer);
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
  <div class="onboarding-card">
    <header class="onboarding-card-header">
      <h2 id="onboarding-modal-title" class="onboarding-card-title">
        Apollia — Premier contact
      </h2>
      <ol class="onboarding-steps" aria-label="Progression de l'onboarding">
        {#each Array(TOTAL_TURNS) as _, i}
          <li
            class="onboarding-step"
            class:active={i < turnIndex}
            aria-current={i === turnIndex && !completed ? "step" : undefined}
          >
            {i + 1}
          </li>
        {/each}
        <li class="onboarding-step-check" class:active={completed}>✓</li>
      </ol>
    </header>

    <div class="onboarding-card-body">
      {#if bootstrapping}
        <div class="onboarding-status" data-testid="onboarding-bootstrap">
          <p>Initialisation de l'agent…</p>
        </div>
      {:else if bootstrapError !== null}
        <div class="onboarding-status onboarding-status-error">
          <p>Impossible de démarrer l'onboarding.</p>
          <p class="onboarding-status-detail">{bootstrapError}</p>
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
    height: min(80vh, 720px);
    display: flex;
    flex-direction: column;
    background: hsl(var(--card));
    border: 1px solid hsl(var(--border));
    border-radius: 1rem;
    box-shadow: var(--shadow-elev-3, 0 24px 64px hsl(var(--foreground) / 0.18));
    overflow: hidden;
  }

  .onboarding-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid hsl(var(--border) / 0.5);
  }

  .onboarding-card-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .onboarding-steps {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
  }

  .onboarding-step,
  .onboarding-step-check {
    width: 1.5rem;
    height: 1.5rem;
    border-radius: 999px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid hsl(var(--border));
    background: transparent;
    transition: background 150ms ease, color 150ms ease, border-color 150ms ease;
  }

  .onboarding-step.active {
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground, 0 0% 100%));
    border-color: hsl(var(--primary));
  }

  .onboarding-step-check.active {
    background: hsl(var(--secondary, var(--primary)));
    color: hsl(var(--primary-foreground, 0 0% 100%));
    border-color: hsl(var(--secondary, var(--primary)));
  }

  .onboarding-card-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .onboarding-status {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 2rem;
    color: hsl(var(--muted-foreground));
    font-size: 0.875rem;
  }

  .onboarding-status-error {
    color: hsl(var(--destructive, 0 84% 60%));
  }

  .onboarding-status-detail {
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
  }

  .onboarding-card-footer {
    display: flex;
    justify-content: center;
    padding: 0.75rem 1.25rem 1rem;
    border-top: 1px solid hsl(var(--border) / 0.5);
  }

  .onboarding-skip {
    background: none;
    border: none;
    color: hsl(var(--muted-foreground));
    font-size: 0.8125rem;
    cursor: pointer;
    padding: 0.25rem 0.75rem;
  }

  .onboarding-skip:hover {
    color: hsl(var(--foreground));
    text-decoration: underline;
  }
</style>
