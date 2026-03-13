<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import OnboardingWizard from "./components/onboarding/OnboardingWizard.svelte";
  import { createSSEConnection } from "$lib/stores/sse";
  import { showOnboarding } from "$lib/stores/onboarding";

  let ready = $state(false);

  onMount(() => {
    const cleanup = createSSEConnection();

    invoke<boolean>("check_onboarded")
      .then((onboarded) => {
        showOnboarding.set(!onboarded);
        ready = true;
      })
      .catch(() => {
        // If check fails, show the app normally
        showOnboarding.set(false);
        ready = true;
      });

    return cleanup;
  });

  function handleOnboardingComplete() {
    showOnboarding.set(false);
  }
</script>

{#if !ready}
  <div class="flex h-screen w-screen items-center justify-center bg-background">
    <p class="text-sm text-muted-foreground">Loading...</p>
  </div>
{:else if $showOnboarding}
  <OnboardingWizard onComplete={handleOnboardingComplete} />
{:else}
  <div class="flex h-screen w-screen overflow-hidden">
    <Sidebar />
    <Main />
  </div>
{/if}
