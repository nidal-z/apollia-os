<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Sparkles, ArrowRight, Clock } from "lucide-svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { LoadingSpinner } from "$lib/components/feedback";

  let starting = $state(false);

  async function startOnboarding(): Promise<void> {
    if (starting) return;
    starting = true;
    try {
      // Reset onboarding state so App.svelte re-enters the onboarding flow
      // with the proper OnboardingConversation component (topic progress bar).
      await invoke("reset_onboarding");
      onboardingStore.setRequired();
      // Force a full page reload to re-trigger App.svelte's onMount logic,
      // which checks onboarding status and shows the conversation phase.
      window.location.reload();
    } catch {
      starting = false;
    }
  }

  function skipOnboarding(): void {
    onboardingStore.setSkipped();
    navigateTo("dashboard");
  }
</script>

<div
  class="flex min-h-[calc(100vh-5rem)] items-center justify-center"
  data-testid="onboarding-screen"
>
  <div class="animate-fade-in flex w-full max-w-lg flex-col items-center">
    <!-- Logo with glow pulse -->
    <div
      class="mb-8 flex h-20 w-20 items-center justify-center rounded-2xl bg-gradient-to-br from-primary to-secondary animate-glow-pulse"
      data-testid="onboarding-logo"
    >
      <Sparkles size={36} strokeWidth={1.5} class="text-white" />
    </div>

    <!-- Glass card -->
    <div class="glass-card glass-border w-full rounded-2xl p-10 text-center">
      <h1
        class="mb-4 text-display-xl text-foreground"
        data-testid="onboarding-title"
      >
        {$t("onboarding_welcome.title")}
      </h1>
      <p class="mb-8 text-base leading-relaxed text-muted-foreground">
        {$t("onboarding_welcome.subtitle")}
      </p>

      <div class="flex flex-col gap-3">
        <button
          class="inline-flex w-full items-center justify-center gap-2 rounded-xl bg-primary px-6 py-3 text-sm font-semibold text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
          data-testid="onboarding-configure"
          onclick={startOnboarding}
          disabled={starting}
        >
          {#if starting}
            <LoadingSpinner size={16} tone="current" />
            {$t("onboarding_welcome.starting")}
          {:else}
            <ArrowRight size={16} strokeWidth={2} />
            {$t("onboarding_welcome.configure")}
          {/if}
        </button>

        <button
          class="inline-flex w-full items-center justify-center gap-2 rounded-xl border border-border bg-transparent px-6 py-3 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          data-testid="onboarding-skip"
          onclick={skipOnboarding}
        >
          <Clock size={16} strokeWidth={1.75} />
          {$t("onboarding_welcome.skip")}
        </button>
      </div>
    </div>
  </div>
</div>
