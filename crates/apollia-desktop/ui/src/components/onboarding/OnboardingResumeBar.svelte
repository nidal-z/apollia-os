<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { ChevronDown, ChevronUp } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  /** Persistence key for the user-collapsed state. */
  const COLLAPSED_KEY = "apollia.onboarding.resume_bar.collapsed";

  interface Props {
    /** Index of the current onboarding phase (1-based). */
    currentStep: number;
    /** Total number of onboarding phases. */
    totalSteps: number;
    /** Called when the user clicks the resume button. */
    onresume: () => void;
    /** Called when the user dismisses the bar for this session. */
    ondismiss: () => void;
  }

  let { currentStep, totalSteps, onresume, ondismiss }: Props = $props();

  let collapsed = $state(false);

  onMount(() => {
    try {
      collapsed = localStorage.getItem(COLLAPSED_KEY) === "true";
    } catch {
      // Storage disabled — default to expanded.
    }
  });

  function toggleCollapsed(): void {
    collapsed = !collapsed;
    try {
      localStorage.setItem(COLLAPSED_KEY, collapsed ? "true" : "false");
    } catch {
      // No-op.
    }
  }
</script>

<div class="resume-bar" class:is-collapsed={collapsed} data-testid="onboarding-resume-bar" role="banner">
  <span class="step-label">
    {$t("onboarding_v2.resume_bar.step_label", { values: { current: currentStep, total: totalSteps } })}
  </span>
  <div class="actions">
    {#if !collapsed}
      <Button variant="primary-gradient" size="sm" onclick={onresume}>
        {$t("onboarding_v2.resume_bar.resume_btn")}
      </Button>
      <Button variant="ghost" size="sm" onclick={ondismiss}>
        {$t("onboarding_v2.resume_bar.dismiss_btn")}
      </Button>
    {/if}
    <button
      class="collapse-btn"
      onclick={toggleCollapsed}
      aria-label={$t("onboarding_v2.resume_bar.toggle_collapsed")}
      aria-expanded={!collapsed}
      data-testid="onboarding-resume-bar-collapse"
    >
      {#if collapsed}
        <ChevronDown size={16} strokeWidth={2} />
      {:else}
        <ChevronUp size={16} strokeWidth={2} />
      {/if}
    </button>
  </div>
</div>

<style>
  .resume-bar {
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1.25rem;
    background: linear-gradient(90deg, hsl(var(--primary) / 0.06) 0%, hsl(var(--secondary) / 0.06) 100%);
    border-bottom: 1px solid hsl(var(--primary) / 0.1);
    flex-shrink: 0;
    gap: 0.75rem;
    transition: height 200ms ease;
  }

  .resume-bar.is-collapsed {
    height: 28px;
  }

  .step-label {
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--foreground) / 0.8);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .collapse-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    border: none;
    background: transparent;
    border-radius: 0.375rem;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
  }

  .collapse-btn:hover {
    background: hsl(var(--muted-foreground) / 0.08);
  }
</style>
