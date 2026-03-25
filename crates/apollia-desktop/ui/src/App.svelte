<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { isLoading } from "svelte-i18n";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import OnboardingLlmSetup from "./components/onboarding/OnboardingLlmSetup.svelte";
  import OnboardingConversation from "./components/onboarding/OnboardingConversation.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import ExtractionNotifier from "./components/chat/ExtractionNotifier.svelte";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { initTheme } from "$lib/stores/theme";
  import type { OnboardingStatus } from "$lib/types";

  /** Onboarding steps: llm-setup → conversation → done */
  type OnboardingStep = "llm-setup" | "conversation" | "done";

  let ready = $state(false);
  let onboardingStep = $state<OnboardingStep>("done");

  onMount(async () => {
    initTheme();
    const cleanup = createSSEConnection();

    try {
      const status = await invoke<OnboardingStatus>("get_onboarding_status");
      if (status.completed || status.skipped) {
        onboardingStep = "done";
      } else {
        // Check if an LLM is already configured
        const llmReady = await invoke<boolean>("check_llm_configured");
        onboardingStep = llmReady ? "conversation" : "llm-setup";
      }
    } catch {
      onboardingStep = "done";
    }
    ready = true;

    return cleanup;
  });

  function handleLlmSetupComplete() {
    onboardingStep = "conversation";
  }

  function handleSkipAll() {
    invoke("dismiss_onboarding").catch(() => {});
    onboardingStep = "done";
  }

  function handleOnboardingComplete() {
    onboardingStep = "done";
  }
</script>

<Tooltip.Provider delayDuration={200}>
  {#if $isLoading || !ready}
    <div class="flex h-screen w-screen items-center justify-center bg-background text-foreground" data-testid="app-loading">
      <p class="text-sm text-muted-foreground">Loading…</p>
    </div>
  {:else if onboardingStep === "llm-setup"}
    <OnboardingLlmSetup
      oncomplete={handleLlmSetupComplete}
      onskip={handleSkipAll}
    />
  {:else if onboardingStep === "conversation"}
    <OnboardingConversation oncomplete={handleOnboardingComplete} />
  {:else}
    <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
      <Sidebar />
      <Main />
    </div>
    <ExtractionNotifier />
    <ToastContainer />
  {/if}
</Tooltip.Provider>
