<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Sparkles, User, Zap, Clock, Bot, MessageSquare, Settings } from "lucide-svelte";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { navigateTo } from "$lib/stores/navigation";
  import type { Route } from "$lib/stores/navigation";

  let showConfetti = $state(false);
  let companionEnabled = $state(false);
  let advancing = $state(false);

  const stats = $derived({
    profile: $onboardingStore.profile ?? "operator",
    featuresExplored: $onboardingStore.tour_step_index,
    actionsCompleted: $onboardingStore.stats.actions_completed,
    totalTimeMinutes: Math.max(1, Math.round($onboardingStore.stats.total_time_sec / 60)),
  });

  const profileLabel = $derived(
    stats.profile === "builder"
      ? $t("onboarding_v2.graduation.profile_builder")
      : $t("onboarding_v2.graduation.profile_operator"),
  );

  onMount(() => {
    showConfetti = true;
  });

  async function handleCommencer(): Promise<void> {
    if (advancing) return;
    advancing = true;
    try {
      await onboardingStore.advancePhase("done");
    } catch {
      advancing = false;
    }
  }

  async function handleCompanionToggle(): Promise<void> {
    try {
      await invoke("set_companion_enabled", { enabled: companionEnabled });
    } catch {
      // Best-effort — preference is re-settable at any time
    }
  }

  function handleCardClick(route: Route): void {
    navigateTo(route);
  }
</script>

<div class="graduation-root" data-testid="onboarding-graduation">
  <!-- Header -->
  <header class="graduation-header">
    <div class="graduation-logo">
      <Sparkles size={28} strokeWidth={1.5} class="text-white" />
    </div>
    <h1 class="graduation-title">{$t("onboarding_v2.graduation.title")}</h1>
    <p class="graduation-subtitle">{$t("onboarding_v2.graduation.subtitle")}</p>
  </header>

  <!-- Stats card -->
  <div class="stats-card" data-testid="graduation-stats">
    <!-- Profile row -->
    <div class="stat-row stat-row--profile">
      <span class="stat-icon"><User size={18} strokeWidth={1.75} /></span>
      <span class="stat-label">{$t("onboarding_v2.graduation.profile_label")}</span>
      <span class="stat-value stat-value--profile" data-testid="graduation-profile">{profileLabel}</span>
    </div>

    <div class="stat-divider"></div>

    <!-- Numeric stats -->
    <div class="stat-grid">
      <div class="stat-cell" data-testid="graduation-features">
        <Zap size={18} strokeWidth={1.75} class="stat-cell-icon" />
        <span class="stat-cell-number">{stats.featuresExplored}</span>
        <span class="stat-cell-label">{$t("onboarding_v2.graduation.stat_features")}</span>
      </div>
      <div class="stat-cell" data-testid="graduation-actions">
        <Sparkles size={18} strokeWidth={1.75} class="stat-cell-icon" />
        <span class="stat-cell-number">{stats.actionsCompleted}</span>
        <span class="stat-cell-label">{$t("onboarding_v2.graduation.stat_actions")}</span>
      </div>
      <div class="stat-cell" data-testid="graduation-time">
        <Clock size={18} strokeWidth={1.75} class="stat-cell-icon" />
        <span class="stat-cell-number">{stats.totalTimeMinutes}</span>
        <span class="stat-cell-label">{$t("onboarding_v2.graduation.stat_time")} ({$t("onboarding_v2.graduation.stat_time_unit")})</span>
      </div>
    </div>
  </div>

  <!-- Quick access cards -->
  <div class="quick-cards" data-testid="graduation-quick-cards">
    <button
      class="quick-card"
      data-testid="graduation-card-agents"
      onclick={() => handleCardClick("agents")}
    >
      <Bot size={22} strokeWidth={1.5} class="quick-card-icon" />
      <span class="quick-card-title">{$t("onboarding_v2.graduation.card_agents")}</span>
      <span class="quick-card-desc">{$t("onboarding_v2.graduation.card_agents_desc")}</span>
    </button>
    <button
      class="quick-card"
      data-testid="graduation-card-chat"
      onclick={() => handleCardClick("chat")}
    >
      <MessageSquare size={22} strokeWidth={1.5} class="quick-card-icon" />
      <span class="quick-card-title">{$t("onboarding_v2.graduation.card_chat")}</span>
      <span class="quick-card-desc">{$t("onboarding_v2.graduation.card_chat_desc")}</span>
    </button>
    <button
      class="quick-card"
      data-testid="graduation-card-settings"
      onclick={() => handleCardClick("settings")}
    >
      <Settings size={22} strokeWidth={1.5} class="quick-card-icon" />
      <span class="quick-card-title">{$t("onboarding_v2.graduation.card_settings")}</span>
      <span class="quick-card-desc">{$t("onboarding_v2.graduation.card_settings_desc")}</span>
    </button>
  </div>

  <!-- Companion toggle -->
  <label class="companion-toggle" data-testid="graduation-companion-toggle">
    <div class="companion-toggle-text">
      <span class="companion-toggle-label">{$t("onboarding_v2.graduation.companion_toggle")}</span>
      <span class="companion-toggle-desc">{$t("onboarding_v2.graduation.companion_desc")}</span>
    </div>
    <input
      type="checkbox"
      class="sr-only"
      bind:checked={companionEnabled}
      onchange={handleCompanionToggle}
      data-testid="graduation-companion-checkbox"
    />
    <div class="toggle-track" class:toggle-track--on={companionEnabled}>
      <div class="toggle-thumb" class:toggle-thumb--on={companionEnabled}></div>
    </div>
  </label>

  <!-- CTA -->
  <button
    class="cta-btn"
    data-testid="graduation-cta"
    onclick={handleCommencer}
    disabled={advancing}
  >
    {#if advancing}
      <span class="cta-spinner"></span>
    {/if}
    {$t("onboarding_v2.graduation.cta")}
  </button>

  <!-- Confetti -->
  {#if showConfetti}
    <div class="confetti-overlay" aria-hidden="true">
      {#each Array(28) as _, i}
        <span
          class="confetti-particle"
          style="--x: {Math.random() * 100}vw; --delay: {Math.random() * 0.8}s; --hue: {i % 3 === 0 ? 240 : i % 3 === 1 ? 260 : 40};"
        ></span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .graduation-root {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: #FFF8F0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1.25rem;
    padding: 2rem 1rem;
    overflow-y: auto;
  }

  /* ── Header ──────────────────────────────────────────────────────────── */

  .graduation-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    text-align: center;
  }

  .graduation-logo {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 1rem;
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--shadow-primary-md), var(--shadow-primary-xl);
    animation: logo-pop 0.4s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }

  .graduation-title {
    font-size: 1.625rem;
    font-weight: 700;
    color: #1a1a2e;
    margin: 0;
  }

  .graduation-subtitle {
    font-size: 0.875rem;
    color: #6B7280;
    margin: 0;
  }

  /* ── Stats card ──────────────────────────────────────────────────────── */

  .stats-card {
    width: 100%;
    max-width: 480px;
    background: rgba(255, 255, 255, 0.8);
    backdrop-filter: blur(20px);
    border: 1px solid rgba(52, 53, 245, 0.08);
    border-radius: 1rem;
    padding: 1.25rem;
    box-shadow: var(--shadow-elev-2);
    animation: card-in 0.35s ease 0.1s both;
  }

  .stat-row {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    font-size: 0.875rem;
  }

  .stat-icon {
    color: #7c5fd6;
    flex-shrink: 0;
  }

  .stat-label {
    color: #6B7280;
    flex: 1;
  }

  .stat-value--profile {
    font-weight: 600;
    color: #3435f5;
    font-size: 0.875rem;
  }

  .stat-divider {
    height: 1px;
    background: rgba(52, 53, 245, 0.06);
    margin: 0.875rem 0;
  }

  .stat-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.75rem;
  }

  .stat-cell {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    text-align: center;
  }

  :global(.stat-cell-icon) {
    color: #7c5fd6;
  }

  .stat-cell-number {
    font-size: 1.375rem;
    font-weight: 700;
    color: #1a1a2e;
    line-height: 1;
  }

  .stat-cell-label {
    font-size: 0.6875rem;
    color: #9CA3AF;
    line-height: 1.3;
  }

  /* ── Quick access cards ──────────────────────────────────────────────── */

  .quick-cards {
    display: flex;
    gap: 0.625rem;
    width: 100%;
    max-width: 480px;
    animation: card-in 0.35s ease 0.2s both;
  }

  .quick-card {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.375rem;
    padding: 0.875rem 0.5rem;
    border-radius: 0.875rem;
    background: rgba(255, 255, 255, 0.72);
    border: 1px solid rgba(52, 53, 245, 0.08);
    cursor: pointer;
    transition: transform 150ms ease, box-shadow 150ms ease, border-color 150ms ease;
    text-align: center;
  }

  .quick-card:hover {
    transform: translateY(-2px);
    box-shadow: var(--shadow-elev-3);
    border-color: rgba(52, 53, 245, 0.18);
  }

  :global(.quick-card-icon) {
    color: #3435f5;
  }

  .quick-card-title {
    font-size: 0.8125rem;
    font-weight: 600;
    color: #1a1a2e;
  }

  .quick-card-desc {
    font-size: 0.6875rem;
    color: #9CA3AF;
    line-height: 1.3;
  }

  /* ── Companion toggle ────────────────────────────────────────────────── */

  .companion-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    width: 100%;
    max-width: 480px;
    padding: 0.875rem 1rem;
    background: rgba(255, 255, 255, 0.72);
    border: 1px solid rgba(52, 53, 245, 0.08);
    border-radius: 0.875rem;
    cursor: pointer;
    animation: card-in 0.35s ease 0.25s both;
    user-select: none;
  }

  .companion-toggle-text {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }

  .companion-toggle-label {
    font-size: 0.875rem;
    font-weight: 600;
    color: #1a1a2e;
  }

  .companion-toggle-desc {
    font-size: 0.75rem;
    color: #9CA3AF;
  }

  .toggle-track {
    width: 2.5rem;
    height: 1.375rem;
    border-radius: 9999px;
    background: #E5E7EB;
    position: relative;
    flex-shrink: 0;
    transition: background 200ms ease;
  }

  .toggle-track--on {
    background: #3435f5;
  }

  .toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 1.125rem;
    height: 1.125rem;
    border-radius: 9999px;
    background: white;
    box-shadow: var(--shadow-elev-1);
    transition: transform 200ms ease;
  }

  .toggle-thumb--on {
    transform: translateX(1.125rem);
  }

  /* ── CTA ─────────────────────────────────────────────────────────────── */

  .cta-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 2.5rem;
    border-radius: 0.875rem;
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    color: white;
    font-size: 0.9375rem;
    font-weight: 600;
    border: none;
    cursor: pointer;
    box-shadow: var(--shadow-primary-md);
    transition: transform 150ms ease, box-shadow 150ms ease, opacity 150ms ease;
    animation: card-in 0.35s ease 0.3s both;
  }

  .cta-btn:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: var(--shadow-primary-lg);
  }

  .cta-btn:disabled {
    opacity: 0.7;
    cursor: default;
  }

  .cta-spinner {
    width: 1rem;
    height: 1rem;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.35);
    border-top-color: white;
    animation: spin 0.7s linear infinite;
  }

  /* ── Confetti ────────────────────────────────────────────────────────── */

  .confetti-overlay {
    position: fixed;
    inset: 0;
    pointer-events: none;
    z-index: 100;
    overflow: hidden;
    animation: confetti-fade 2.5s ease forwards;
  }

  .confetti-particle {
    position: absolute;
    top: -8px;
    left: var(--x);
    width: 7px;
    height: 7px;
    border-radius: 2px;
    background: hsl(var(--hue), 88%, 56%);
    opacity: 0.7;
    animation: confetti-fall 2.2s ease var(--delay) forwards;
  }

  /* ── Keyframes ───────────────────────────────────────────────────────── */

  @keyframes logo-pop {
    from { transform: scale(0.7); opacity: 0; }
    to   { transform: scale(1);   opacity: 1; }
  }

  @keyframes card-in {
    from { transform: translateY(12px); opacity: 0; }
    to   { transform: translateY(0);    opacity: 1; }
  }

  @keyframes confetti-fall {
    0%   { transform: translateY(0) rotate(0deg);    opacity: 0.7; }
    100% { transform: translateY(100vh) rotate(720deg); opacity: 0; }
  }

  @keyframes confetti-fade {
    0%, 75% { opacity: 1; }
    100%    { opacity: 0; }
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border-width: 0;
  }
</style>
