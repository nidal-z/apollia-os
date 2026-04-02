<script lang="ts">
  import { onMount } from "svelte";
  import { isLoading } from "svelte-i18n";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import OnboardingWelcome from "./components/onboarding/OnboardingWelcome.svelte";
  import ProfileSelector from "./components/onboarding/ProfileSelector.svelte";
  import OnboardingAiSetup from "./components/onboarding/OnboardingAiSetup.svelte";
  import OnboardingAcquaintance from "./components/onboarding/OnboardingAcquaintance.svelte";
  import OnboardingGuidedTour from "./components/onboarding/OnboardingGuidedTour.svelte";
  import OnboardingGraduation from "./components/onboarding/OnboardingGraduation.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import ExtractionNotifier from "./components/chat/ExtractionNotifier.svelte";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { initTheme } from "$lib/stores/theme";
  import { onboardingStore } from "$lib/stores/onboarding";

  let ready = $state(false);

  onMount(() => {
    initTheme();
    const cleanup = createSSEConnection();

    onboardingStore
      .refreshState()
      .catch(() => {
        // Backend unreachable at startup — default state (phase: "welcome") is used.
      })
      .finally(() => {
        ready = true;
      });

    return cleanup;
  });
</script>

<Tooltip.Provider delayDuration={200}>
  {#if $isLoading || !ready}
    <div
      class="flex h-screen w-screen items-center justify-center bg-background text-foreground"
      data-testid="app-loading"
    >
      <p class="text-sm text-muted-foreground">Loading…</p>
    </div>
  {:else if $onboardingStore.completed || $onboardingStore.phase === "done"}
    <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
      <Sidebar />
      <Main />
    </div>
    <ExtractionNotifier />
    <ToastContainer />
  {:else if $onboardingStore.phase === "welcome"}
    <OnboardingWelcome />
  {:else if $onboardingStore.phase === "profile_choice"}
    <ProfileSelector />
  {:else if $onboardingStore.phase === "ai_setup"}
    <OnboardingAiSetup />
  {:else if $onboardingStore.phase === "acquaintance"}
    <OnboardingAcquaintance />
  {:else if $onboardingStore.phase === "guided_tour"}
    <OnboardingGuidedTour />
  {:else if $onboardingStore.phase === "graduation"}
    <OnboardingGraduation />
  {:else}
    <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
      <Sidebar />
      <Main />
    </div>
    <ExtractionNotifier />
    <ToastContainer />
  {/if}
</Tooltip.Provider>
