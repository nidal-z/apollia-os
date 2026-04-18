<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { isLoading } from "svelte-i18n";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import OnboardingWelcome from "./components/onboarding/OnboardingWelcome.svelte";
  import ProfileSelector from "./components/onboarding/ProfileSelector.svelte";
  import OnboardingAiSetup from "./components/onboarding/OnboardingAiSetup.svelte";
  import OnboardingAcquaintance from "./components/onboarding/OnboardingAcquaintance.svelte";
  import OnboardingGuidedTour from "./components/onboarding/OnboardingGuidedTour.svelte";
  import OnboardingGraduation from "./components/onboarding/OnboardingGraduation.svelte";
  import OnboardingResumeBar from "./components/onboarding/OnboardingResumeBar.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import ExtractionNotifier from "./components/chat/ExtractionNotifier.svelte";
  import CompanionContextProvider from "./components/companion/CompanionContextProvider.svelte";
  import CompanionPanel from "./components/companion/CompanionPanel.svelte";
  import { companionStore } from "$lib/stores/companion";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { initTheme } from "$lib/stores/theme";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { navigateTo } from "$lib/stores/navigation";
  import type { OnboardingPhase } from "$lib/types";

  // Phases that represent an in-progress (but not yet started) onboarding.
  const IN_PROGRESS_PHASES = new Set<OnboardingPhase>([
    "profile_choice",
    "ai_setup",
    "acquaintance",
    "guided_tour",
    "graduation",
  ]);

  // Step numbers for the resume bar label (1-based, 5 phases after welcome).
  const PHASE_STEP: Partial<Record<OnboardingPhase, number>> = {
    profile_choice: 1,
    ai_setup: 2,
    acquaintance: 3,
    guided_tour: 4,
    graduation: 5,
  };
  const TOTAL_STEPS = 5;

  type OnboardingDisplayMode =
    | "phase"        // Active onboarding — render the phase component fullscreen.
    | "main_with_bar" // Resumed at startup — show main app + non-intrusive bar.
    | "main";        // User dismissed the bar — show main app without bar.

  let ready = $state(false);
  let displayMode = $state<OnboardingDisplayMode>("phase");

  onMount(() => {
    initTheme();
    const cleanup = createSSEConnection();

    // Design showcase pages — internal-facing, not advertised in the sidebar.
    if (typeof window !== "undefined") {
      if (window.location.hash === "#design") {
        navigateTo("design");
      } else if (window.location.hash === "#motion") {
        navigateTo("design-motion");
      } else if (window.location.hash === "#design-empty-states") {
        navigateTo("design-empty-states");
      }
    }

    onboardingStore
      .refreshState()
      .catch(() => {
        // Backend unreachable at startup — default state (phase: "welcome") is used.
      })
      .finally(() => {
        ready = true;
        const { phase } = get(onboardingStore);
        if (IN_PROGRESS_PHASES.has(phase)) {
          displayMode = "main_with_bar";
        }
      });

    void companionStore.initFromMemory();

    return cleanup;
  });

  function handleResume(): void {
    displayMode = "phase";
  }

  function handleDismissBar(): void {
    displayMode = "main";
  }
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
    <CompanionContextProvider>
      <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
        <Sidebar />
        <Main />
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <ExtractionNotifier />
    <ToastContainer />
  {:else if $onboardingStore.phase === "welcome"}
    <OnboardingWelcome />
  {:else if (displayMode === "main_with_bar" || displayMode === "main") && IN_PROGRESS_PHASES.has($onboardingStore.phase)}
    <CompanionContextProvider>
      <div class="flex h-screen w-screen flex-col overflow-hidden" data-testid="app-main">
        {#if displayMode === "main_with_bar"}
          <OnboardingResumeBar
            currentStep={PHASE_STEP[$onboardingStore.phase] ?? 1}
            totalSteps={TOTAL_STEPS}
            onresume={handleResume}
            ondismiss={handleDismissBar}
          />
        {/if}
        <div class="flex min-h-0 flex-1 overflow-hidden">
          <Sidebar />
          <Main />
        </div>
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <ExtractionNotifier />
    <ToastContainer />
  {:else if $onboardingStore.phase === "profile_choice"}
    <ProfileSelector />
  {:else if $onboardingStore.phase === "ai_setup"}
    <OnboardingAiSetup />
  {:else if $onboardingStore.phase === "acquaintance"}
    <OnboardingAcquaintance />
  {:else if $onboardingStore.phase === "guided_tour"}
    <CompanionContextProvider>
      <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
        <Sidebar />
        <Main />
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <OnboardingGuidedTour />
  {:else if $onboardingStore.phase === "graduation"}
    <OnboardingGraduation />
  {:else}
    <CompanionContextProvider>
      <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
        <Sidebar />
        <Main />
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <ExtractionNotifier />
    <ToastContainer />
  {/if}
</Tooltip.Provider>
