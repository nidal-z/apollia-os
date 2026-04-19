<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Sparkles } from "lucide-svelte";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import TopicProgressBar from "./TopicProgressBar.svelte";
  import type { OnboardingStatus, TriggerResult } from "$lib/types";

  interface Props {
    oncomplete: () => void;
  }

  let { oncomplete }: Props = $props();

  let sessionId = $state<string | null>(null);
  let isLoading = $state(true);
  let isCompleted = $state(false);
  let showConfetti = $state(false);
  let topicsCovered = $state<string[]>([]);
  let activeTopic = $state<string | null>(null);
  let fadeIn = $state(false);

  const ALL_TOPICS = ["identity", "preferences", "tools", "domain", "agents"];
  const POLL_INTERVAL_MS = 4000;
  let pollTimer: ReturnType<typeof setInterval> | undefined;

  onMount(async () => {
    try {
      const result: TriggerResult = await invoke("trigger_onboarding", { topic: null });
      sessionId = result.session_id;
      activeTopic = ALL_TOPICS[0];
      isLoading = false;
      requestAnimationFrame(() => { fadeIn = true; });

      pollTimer = setInterval(pollTopicProgress, POLL_INTERVAL_MS);

      // Auto-send a greeting to kick off the conversation — the onboarding
      // agent will respond with a personalised welcome, so the user isn't
      // staring at an empty chat wondering what to type.
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
        activeTopic = null;
        return;
      }

      const nextUncovered = ALL_TOPICS.find((t) => !status.topics_covered.includes(t));
      activeTopic = nextUncovered ?? null;
    } catch {
      // Polling failure is non-critical — retry on next interval
    }
  }

  async function handleSkip(): Promise<void> {
    try {
      await invoke("dismiss_onboarding");
    } catch {
      // Best effort
    }
    oncomplete();
  }

  function handleStartClick(): void {
    oncomplete();
  }

  function noop(): void {
    // Embedded chat does not need a close handler
  }
</script>

<div
  class="onboarding-fullscreen"
  class:fade-in={fadeIn}
  data-testid="onboarding-conversation"
>
  <header class="onboarding-header" data-testid="onboarding-header">
    <div class="onboarding-logo-wrap">
      <Sparkles size={28} strokeWidth={1.5} class="text-white" />
    </div>
    <h1 class="onboarding-title">{$t("onboarding_welcome.welcome_heading")}</h1>
    <p class="onboarding-subtitle">{$t("onboarding_welcome.welcome_subheading")}</p>
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
        <p>{$t("onboarding_welcome.start_failed")}</p>
        <button
          class="onboarding-error-btn"
          onclick={handleSkip}
        >
          {$t("onboarding_welcome.continue_to_dashboard")}
        </button>
      </div>
    {/if}

    {#if isCompleted}
      <div class="onboarding-completion">
        <p class="onboarding-completion-text">
          {$t("onboarding_welcome.completion_text")}
        </p>
        <button
          class="onboarding-start-btn"
          data-testid="onboarding-start-btn"
          onclick={handleStartClick}
        >
          {$t("onboarding_welcome.start")}
        </button>
      </div>
    {/if}
  </main>

  <footer class="onboarding-footer">
    <TopicProgressBar {topicsCovered} {activeTopic} />
    {#if !isCompleted}
      <button
        class="btn-skip"
        data-testid="onboarding-skip-btn"
        onclick={handleSkip}
      >
        {$t("onboarding_welcome.skip_for_now")}
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
    background: #FFF8F0;
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
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--shadow-primary-md), var(--shadow-primary-xl);
  }

  .onboarding-title {
    font-size: 1.5rem;
    font-weight: 700;
    color: #1a1a2e;
    margin: 0;
  }

  .onboarding-subtitle {
    font-size: 0.875rem;
    color: #6B7280;
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
    background: rgba(255, 255, 255, 0.72);
    backdrop-filter: blur(20px);
    border: 1px solid rgba(52, 53, 245, 0.06);
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
    color: #6B7280;
    font-size: 0.875rem;
  }

  .onboarding-error-btn {
    padding: 0.5rem 1.25rem;
    border-radius: 0.75rem;
    background: #3435f5;
    color: white;
    font-size: 0.875rem;
    font-weight: 600;
    border: none;
    cursor: pointer;
    transition: background 150ms ease;
  }

  .onboarding-error-btn:hover {
    background: #2a2bd4;
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
    color: #7c5fd6;
    margin: 0;
  }

  .onboarding-start-btn {
    padding: 0.625rem 2rem;
    border-radius: 0.75rem;
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
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
    color: #6B7280;
    font-size: 0.8125rem;
    cursor: pointer;
    text-decoration: none;
    padding: 0.25rem 0.5rem;
    transition: color 150ms ease;
  }

  .btn-skip:hover {
    text-decoration: underline;
    color: #4B5563;
  }

  /* Confetti animation */
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
    0%, 80% { opacity: 1; }
    100% { opacity: 0; }
  }
</style>
