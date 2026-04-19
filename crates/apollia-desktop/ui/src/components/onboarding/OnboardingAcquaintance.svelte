<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { Sparkles } from "lucide-svelte";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import ProgressRing from "./ProgressRing.svelte";
  import { onboardingStore } from "$lib/stores/onboarding";
  import type { OnboardingStatus, TriggerResult } from "$lib/types";

  const ALL_TOPICS = ["identity", "preferences", "tools", "domain", "agents"] as const;
  const POLL_INTERVAL_MS = 4000;

  let sessionId = $state<string | null>(null);
  let isLoading = $state(true);
  let isCompleted = $state(false);
  let showConfetti = $state(false);
  let topicsCovered = $state<string[]>([]);
  let fadeIn = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | undefined;

  const completedCount = $derived(topicsCovered.length);

  function noop(): void {
    // Embedded chat does not need a close handler.
  }

  onMount(async () => {
    const state = get(onboardingStore);
    const profile: string | null = state.profile;

    try {
      const result: TriggerResult = await invoke("trigger_onboarding", {
        topic: null,
        profile,
      });
      sessionId = result.session_id;
      isLoading = false;
      requestAnimationFrame(() => {
        fadeIn = true;
      });

      pollTimer = setInterval(pollTopicProgress, POLL_INTERVAL_MS);

      await invoke("send_chat_message", {
        sessionId: result.session_id,
        content: "Bonjour !",
      });
    } catch {
      isLoading = false;
      fadeIn = true;
    }
  });

  onDestroy(() => {
    if (pollTimer !== undefined) {
      clearInterval(pollTimer);
    }
  });

  async function pollTopicProgress(): Promise<void> {
    try {
      const status: OnboardingStatus = await invoke("get_onboarding_status");
      topicsCovered = status.topics_covered;

      if (status.completed) {
        isCompleted = true;
        showConfetti = true;
        if (pollTimer !== undefined) {
          clearInterval(pollTimer);
          pollTimer = undefined;
        }
        return;
      }
    } catch {
      // Non-critical — retry on next interval.
    }
  }

  async function handleSkip(): Promise<void> {
    try {
      await invoke("dismiss_onboarding");
    } catch {
      // Best effort.
    }
    await onboardingStore.advancePhase("guided_tour").catch(() => {});
  }

  async function handleContinue(): Promise<void> {
    await onboardingStore.advancePhase("guided_tour").catch(() => {});
  }
</script>

<div
  class="onboarding-fullscreen"
  class:fade-in={fadeIn}
  data-testid="onboarding-acquaintance"
>
  <header class="onboarding-header">
    <div class="onboarding-logo-wrap">
      <Sparkles size={28} strokeWidth={1.5} class="text-white" />
    </div>
    <h1 class="onboarding-title">{$t("onboarding_v2.acquaintance.title")}</h1>
    <p class="onboarding-subtitle">{$t("onboarding_v2.acquaintance.subtitle")}</p>
  </header>

  <main class="onboarding-chat-area">
    {#if sessionId}
      <div class="onboarding-chat-card">
        <ChatConversation
          sessionId={sessionId}
          onclose={noop}
          embedded={true}
          hideConfig={true}
        />
      </div>
    {:else if !isLoading}
      <div class="onboarding-error">
        <p>{$t("onboarding_v2.acquaintance.error")}</p>
        <button class="onboarding-error-btn" onclick={handleSkip}>
          {$t("onboarding_v2.acquaintance.continue_dashboard")}
        </button>
      </div>
    {/if}

    {#if isCompleted}
      <div class="onboarding-completion">
        <p class="onboarding-completion-text">
          {$t("onboarding_v2.acquaintance.completed_text")}
        </p>
        <button
          class="onboarding-start-btn"
          data-testid="onboarding-start-btn"
          onclick={handleContinue}
        >
          {$t("onboarding_v2.acquaintance.continue")}
        </button>
      </div>
    {/if}
  </main>

  <footer class="onboarding-footer">
    <ProgressRing
      segments={ALL_TOPICS.length}
      completed={completedCount}
      size={72}
      strokeWidth={8}
    />
    {#if !isCompleted}
      <button class="btn-skip" data-testid="onboarding-skip-btn" onclick={handleSkip}>
        {$t("onboarding_v2.acquaintance.skip")}
      </button>
    {/if}
  </footer>

  {#if showConfetti}
    <div class="confetti-overlay" aria-hidden="true">
      {#each Array(24) as _, i}
        <span
          class="confetti-particle"
          style="--x: {Math.random() * 100}vw; --delay: {Math.random() * 0.8}s; --hue: {i % 2 === 0 ? 240 : 260};"
        ></span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .onboarding-fullscreen {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: hsl(var(--background));
    display: flex;
    flex-direction: column;
    align-items: center;
    opacity: 0;
    transition: opacity 300ms ease-in;
  }

  .onboarding-fullscreen.fade-in {
    opacity: 1;
  }

  .onboarding-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 2.5rem 1rem 1rem;
    gap: 0.5rem;
  }

  .onboarding-logo-wrap {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 1rem;
    background: var(--gradient-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--shadow-primary-md), var(--shadow-primary-xl);
  }

  .onboarding-title {
    font-size: 1.5rem;
    font-weight: 700;
    color: hsl(var(--foreground));
    margin: 0;
  }

  .onboarding-subtitle {
    font-size: 0.875rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
  }

  .onboarding-chat-area {
    flex: 1;
    width: 100%;
    max-width: 672px;
    padding: 0 1rem;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-height: 0;
  }

  .onboarding-chat-card {
    flex: 1;
    min-height: 0;
    border-radius: 1rem;
    background: hsl(var(--card) / 0.72);
    backdrop-filter: blur(20px);
    border: 1px solid hsl(var(--primary) / 0.06);
    box-shadow: var(--shadow-elev-2);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .onboarding-error {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    padding: 3rem 1rem;
    color: hsl(var(--muted-foreground));
    font-size: 0.875rem;
  }

  .onboarding-error-btn {
    padding: 0.5rem 1.25rem;
    border-radius: 0.75rem;
    background: hsl(var(--primary));
    color: white;
    font-size: 0.875rem;
    font-weight: 600;
    border: none;
    cursor: pointer;
    transition: background 150ms ease;
  }

  .onboarding-error-btn:hover {
    background: hsl(var(--primary));
  }

  .onboarding-completion {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem 0;
  }

  .onboarding-completion-text {
    font-size: 0.9375rem;
    font-weight: 600;
    color: hsl(var(--secondary));
    margin: 0;
  }

  .onboarding-start-btn {
    padding: 0.625rem 2rem;
    border-radius: 0.75rem;
    background: var(--gradient-primary);
    color: white;
    font-size: 0.875rem;
    font-weight: 600;
    border: none;
    cursor: pointer;
    box-shadow: var(--shadow-primary-md);
    transition: transform 150ms ease, box-shadow 150ms ease;
  }

  .onboarding-start-btn:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-primary-lg);
  }

  .onboarding-footer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem 1.5rem;
    width: 100%;
  }

  .btn-skip {
    background: none;
    border: none;
    color: hsl(var(--muted-foreground));
    font-size: 0.8125rem;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    transition: color 150ms ease;
  }

  .btn-skip:hover {
    text-decoration: underline;
    color: hsl(var(--foreground) / 0.8);
  }

  .confetti-overlay {
    position: fixed;
    inset: 0;
    pointer-events: none;
    z-index: 100;
    overflow: hidden;
    animation: confetti-fade 2s ease forwards;
  }

  .confetti-particle {
    position: absolute;
    top: -8px;
    left: var(--x);
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: hsl(var(--hue), 91%, 58%);
    opacity: 0.6;
    animation: confetti-fall 2s ease var(--delay) forwards;
  }

  @keyframes confetti-fall {
    0% {
      transform: translateY(0) rotate(0deg);
      opacity: 0.6;
    }
    100% {
      transform: translateY(100vh) rotate(720deg);
      opacity: 0;
    }
  }

  @keyframes confetti-fade {
    0%,
    80% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
</style>
