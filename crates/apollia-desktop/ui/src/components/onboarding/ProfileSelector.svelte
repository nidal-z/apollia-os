<script lang="ts">
  import { t } from "svelte-i18n";
  import { Terminal, Code2, Loader2 } from "lucide-svelte";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { uiMode } from "$lib/stores/mode";

  type Profile = "operator" | "builder";

  let selecting = $state<Profile | null>(null);
  let error = $state<string | null>(null);
  let visible = $state(false);

  $effect(() => {
    requestAnimationFrame(() => {
      visible = true;
    });
  });

  async function handleSelectProfile(profile: Profile): Promise<void> {
    if (selecting !== null) return;
    selecting = profile;
    error = null;
    try {
      await onboardingStore.setProfile(profile);
      uiMode.set(profile);
      await onboardingStore.advancePhase("ai_setup");
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      selecting = null;
    }
  }
</script>

<div
  class="profile-screen"
  class:visible
  data-testid="onboarding-profile-selector"
>
  <div class="profile-content">
    <header class="profile-header">
      <h1 class="profile-title">{$t("onboarding_v2.profile.title")}</h1>
      <p class="profile-subtitle">{$t("onboarding_v2.profile.subtitle")}</p>
    </header>

    {#if error}
      <p class="profile-error" role="alert">{error}</p>
    {/if}

    <div class="profile-cards">
      <button
        class="profile-card"
        class:is-selecting={selecting === "operator"}
        onclick={() => handleSelectProfile("operator")}
        disabled={selecting !== null}
        data-testid="profile-card-operator"
      >
        <div class="card-icon card-icon-operator">
          {#if selecting === "operator"}
            <Loader2 size={24} strokeWidth={1.75} class="animate-spin" />
          {:else}
            <Terminal size={24} strokeWidth={1.75} />
          {/if}
        </div>
        <h2 class="card-title">{$t("onboarding_v2.profile.operator_title")}</h2>
        <p class="card-desc">{$t("onboarding_v2.profile.operator_desc")}</p>
        <p class="card-examples">{$t("onboarding_v2.profile.operator_examples")}</p>
      </button>

      <button
        class="profile-card"
        class:is-selecting={selecting === "builder"}
        onclick={() => handleSelectProfile("builder")}
        disabled={selecting !== null}
        data-testid="profile-card-builder"
      >
        <div class="card-icon card-icon-builder">
          {#if selecting === "builder"}
            <Loader2 size={24} strokeWidth={1.75} class="animate-spin" />
          {:else}
            <Code2 size={24} strokeWidth={1.75} />
          {/if}
        </div>
        <h2 class="card-title">{$t("onboarding_v2.profile.builder_title")}</h2>
        <p class="card-desc">{$t("onboarding_v2.profile.builder_desc")}</p>
        <p class="card-examples">{$t("onboarding_v2.profile.builder_examples")}</p>
      </button>
    </div>

    <button
      class="both-profiles-link"
      onclick={() => handleSelectProfile("builder")}
      disabled={selecting !== null}
      data-testid="profile-both"
    >
      {$t("onboarding_v2.profile.both_profiles")}
    </button>
  </div>
</div>

<style>
  .profile-screen {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #FFF8F0;
    opacity: 0;
    transition: opacity 350ms ease-in;
  }

  .profile-screen.visible {
    opacity: 1;
  }

  .profile-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.5rem;
    padding: 2rem;
    max-width: 48rem;
    width: 100%;
  }

  .profile-header {
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .profile-title {
    font-size: 1.625rem;
    font-weight: 700;
    color: #1a1a2e;
    margin: 0;
    letter-spacing: -0.02em;
  }

  .profile-subtitle {
    font-size: 0.9375rem;
    color: #6B7280;
    margin: 0;
  }

  .profile-error {
    font-size: 0.8125rem;
    color: #dc2626;
    margin: 0;
    padding: 0.5rem 0.875rem;
    border-radius: 0.5rem;
    background: rgba(220, 38, 38, 0.06);
    border: 1px solid rgba(220, 38, 38, 0.15);
  }

  .profile-cards {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    width: 100%;
  }

  /* Breakpoint Tailwind sm = 640 px — voir src/lib/design/breakpoints.md. */
  @media (min-width: 640px) {
    .profile-cards {
      flex-direction: row;
    }
  }

  .profile-card {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.75rem;
    padding: 1.75rem;
    border-radius: 1.25rem;
    border: 2px solid rgba(52, 53, 245, 0.08);
    background: rgba(255, 255, 255, 0.82);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.5),
      0 2px 8px -2px rgba(0, 0, 0, 0.04),
      0 8px 24px -8px rgba(52, 53, 245, 0.06);
    cursor: pointer;
    text-align: left;
    transition: transform 150ms ease, box-shadow 150ms ease, border-color 150ms ease;
  }

  .profile-card:hover:not(:disabled) {
    transform: translateY(-3px);
    border-color: rgba(52, 53, 245, 0.28);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.6),
      0 4px 16px -2px rgba(0, 0, 0, 0.06),
      0 16px 40px -8px rgba(52, 53, 245, 0.12);
  }

  .profile-card:disabled {
    opacity: 0.65;
    cursor: not-allowed;
  }

  .profile-card.is-selecting {
    border-color: rgba(52, 53, 245, 0.4);
    opacity: 1;
  }

  .card-icon {
    width: 3rem;
    height: 3rem;
    border-radius: 0.75rem;
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
  }

  .card-icon-operator {
    background: linear-gradient(135deg, #3435f5, #5b56f8);
    box-shadow: 0 4px 12px -2px rgba(52, 53, 245, 0.35);
  }

  .card-icon-builder {
    background: linear-gradient(135deg, #7c5fd6, #9b7ff0);
    box-shadow: 0 4px 12px -2px rgba(124, 95, 214, 0.35);
  }

  .card-title {
    font-size: 1.125rem;
    font-weight: 700;
    color: #1a1a2e;
    margin: 0;
  }

  .card-desc {
    font-size: 0.875rem;
    color: #4B5563;
    margin: 0;
    line-height: 1.5;
  }

  .card-examples {
    font-size: 0.8125rem;
    color: #9CA3AF;
    margin: 0;
    font-style: italic;
  }

  .both-profiles-link {
    font-size: 0.8125rem;
    color: #9CA3AF;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    border-radius: 0.25rem;
    transition: color 150ms ease;
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .both-profiles-link:hover:not(:disabled) {
    color: #6B7280;
  }

  .both-profiles-link:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

</style>
