<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { isLoading } from "svelte-i18n";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import OnboardingConversation from "./components/onboarding/OnboardingConversation.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import ExtractionNotifier from "./components/chat/ExtractionNotifier.svelte";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { initTheme } from "$lib/stores/theme";
  import type { OnboardingStatus } from "$lib/types";

  let ready = $state(false);
  let showOnboarding = $state(false);

  onMount(() => {
    initTheme();
    const cleanup = createSSEConnection();

    invoke<OnboardingStatus>("get_onboarding_status")
      .then((status) => {
        showOnboarding = !status.completed && !status.skipped;
        ready = true;
      })
      .catch(() => {
        showOnboarding = false;
        ready = true;
      });

    return cleanup;
  });

  function handleOnboardingComplete() {
    showOnboarding = false;
  }
</script>

<Tooltip.Provider delayDuration={200}>
  {#if $isLoading || !ready}
    <div class="flex h-screen w-screen items-center justify-center bg-background text-foreground" data-testid="app-loading">
      <p class="text-sm text-muted-foreground">Loading…</p>
    </div>
  {:else if showOnboarding}
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
