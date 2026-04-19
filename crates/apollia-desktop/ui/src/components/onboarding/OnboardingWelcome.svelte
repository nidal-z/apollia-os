<script lang="ts">
  import { t } from "svelte-i18n";
  import { Sparkles } from "lucide-svelte";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { Spinner } from "$lib/components/ui/progress";

  let advancing = $state(false);
  let error = $state<string | null>(null);
  let visible = $state(false);

  $effect(() => {
    requestAnimationFrame(() => {
      visible = true;
    });
  });

  async function handleStart(): Promise<void> {
    if (advancing) return;
    advancing = true;
    error = null;
    try {
      await onboardingStore.advancePhase("profile_choice");
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      advancing = false;
    }
  }
</script>

<div
  class="welcome-screen"
  class:visible
  data-testid="onboarding-welcome"
>
  <div class="welcome-content">
    <div class="welcome-logo" aria-hidden="true">
      <Sparkles size={32} strokeWidth={1.5} class="text-white" />
    </div>

    <h1 class="welcome-tagline">{$t("onboarding_v2.welcome.tagline")}</h1>
    <p class="welcome-subtitle">{$t("onboarding_v2.welcome.subtitle")}</p>

    {#if error}
      <p class="welcome-error" role="alert">{error}</p>
    {/if}

    <button
      class="welcome-cta"
      class:loading={advancing}
      onclick={handleStart}
      disabled={advancing}
      data-testid="onboarding-welcome-cta"
    >
      {#if advancing}
        <Spinner size={18} />
        {$t("onboarding_v2.welcome.loading")}
      {:else}
        {$t("onboarding_v2.welcome.cta")}
      {/if}
    </button>
  </div>
</div>

<style>
  .welcome-screen {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, hsl(var(--background)) 0%, hsl(var(--background)) 45%, hsl(var(--background)) 100%);
    opacity: 0;
    transition: opacity 400ms ease-in;
  }

  .welcome-screen.visible {
    opacity: 1;
  }

  .welcome-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.25rem;
    padding: 2rem;
    max-width: 30rem;
    text-align: center;
  }

  .welcome-logo {
    --logo-shadow-rest:
      0 0 0 1px hsl(var(--primary) / 0.20),
      var(--shadow-primary-md),
      var(--shadow-primary-xl);
    --logo-shadow-active:
      0 0 0 1px hsl(var(--primary) / 0.35),
      var(--shadow-primary-lg),
      var(--shadow-primary-xl);
    width: 4.5rem;
    height: 4.5rem;
    border-radius: 1.25rem;
    background: linear-gradient(135deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)));
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--logo-shadow-rest);
    animation: logo-pulse 3s ease-in-out infinite;
  }

  @keyframes logo-pulse {
    0%, 100% { box-shadow: var(--logo-shadow-rest); }
    50%      { box-shadow: var(--logo-shadow-active); }
  }

  .welcome-tagline {
    font-size: 1.75rem;
    font-weight: 700;
    color: white;
    margin: 0;
    line-height: 1.2;
    letter-spacing: -0.02em;
  }

  .welcome-subtitle {
    font-size: 0.9375rem;
    color: hsl(var(--foreground) / 0.55);
    margin: 0;
    line-height: 1.6;
  }

  .welcome-error {
    font-size: 0.8125rem;
    color: hsl(var(--destructive) / 0.85);
    margin: 0;
    padding: 0.5rem 0.875rem;
    border-radius: 0.5rem;
    background: hsl(var(--destructive) / 0.05);
    border: 1px solid hsl(var(--destructive) / 0.15);
  }

  .welcome-cta {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    margin-top: 0.5rem;
    padding: 0.875rem 2.5rem;
    border-radius: 0.875rem;
    border: none;
    background: linear-gradient(135deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)));
    color: hsl(var(--primary-foreground));
    font-size: 0.9375rem;
    font-weight: 600;
    cursor: pointer;
    transition: transform 150ms ease, box-shadow 150ms ease, opacity 150ms ease;
    box-shadow: var(--shadow-primary-md), var(--shadow-primary-xl);
    min-width: 10rem;
  }

  .welcome-cta:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: var(--shadow-primary-lg), var(--shadow-primary-xl);
  }

  .welcome-cta:active:not(:disabled) {
    transform: translateY(0);
  }

  .welcome-cta:disabled {
    opacity: 0.7;
    cursor: not-allowed;
  }
</style>
