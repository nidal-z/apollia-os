<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { isLoading } from "svelte-i18n";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import OnboardingWizard from "./components/onboarding/OnboardingWizard.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import ExtractionNotifier from "./components/chat/ExtractionNotifier.svelte";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { showOnboarding } from "$lib/stores/onboarding";
  import { initTheme } from "$lib/stores/theme";

  let ready = $state(false);

  onMount(() => {
    initTheme();
    const cleanup = createSSEConnection();

    invoke<boolean>("check_onboarded")
      .then((onboarded) => {
        showOnboarding.set(!onboarded);
        ready = true;
      })
      .catch(() => {
        showOnboarding.set(false);
        ready = true;
      });

    return cleanup;
  });

  function handleOnboardingComplete() {
    showOnboarding.set(false);
  }
</script>

<Tooltip.Provider delayDuration={200}>
  {#if $isLoading || !ready}
    <div class="flex h-screen w-screen items-center justify-center bg-background text-foreground" data-testid="app-loading">
      <p class="text-sm text-muted-foreground">Loading…</p>
    </div>
  {:else if $showOnboarding}
    <OnboardingWizard onComplete={handleOnboardingComplete} />
  {:else}
    <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
      <Sidebar />
      <Main />
    </div>
    <ExtractionNotifier />
    <ToastContainer />
  {/if}
</Tooltip.Provider>
