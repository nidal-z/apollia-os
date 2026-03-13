<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { currentRoute } from "$lib/stores/navigation";
  import { showOnboarding } from "$lib/stores/onboarding";
  import { Button } from "$lib/components/ui/button";
  import type { ApollaConfigView } from "$lib/types";

  let configView = $state<ApollaConfigView | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let openingEditor = $state(false);
  let resettingOnboarding = $state(false);

  async function loadConfig() {
    loading = true;
    error = null;
    try {
      configView = await invoke<ApollaConfigView>("get_config");
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function openEditor() {
    openingEditor = true;
    try {
      await invoke("open_config_in_editor");
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      openingEditor = false;
    }
  }

  async function resetOnboarding() {
    resettingOnboarding = true;
    try {
      await invoke("reset_onboarding");
      showOnboarding.set(true);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      resettingOnboarding = false;
    }
  }

  function navigateTo(route: string) {
    currentRoute.set(route as "llm" | "triggers");
  }

  onMount(() => {
    loadConfig();
  });
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="flex items-center gap-3">
      <span class="text-2xl">⚙️</span>
      <h1 class="text-2xl font-bold">Settings</h1>
    </div>
    <Button onclick={openEditor} disabled={openingEditor}>
      {openingEditor ? "Opening..." : "Open in Editor"}
    </Button>
  </div>

  <!-- Info banner (AC-4) -->
  <div class="rounded-md border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-800 dark:border-blue-800 dark:bg-blue-950/50 dark:text-blue-200">
    Configuration en lecture seule. Pour modifier, éditez <code class="rounded bg-blue-100 px-1 font-mono text-xs dark:bg-blue-900">apollia.toml</code> et redémarrez le runtime.
  </div>

  {#if error}
    <div class="rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-4 py-2 text-sm text-[hsl(var(--destructive))]">
      {error}
    </div>
  {/if}

  {#if loading}
    <div class="flex items-center justify-center py-16">
      <p class="text-muted-foreground">Loading configuration...</p>
    </div>
  {:else if configView}
    <!-- Config file path -->
    <div class="text-xs text-muted-foreground">
      <span class="font-mono">{configView.config_path}</span>
      {#if !configView.config_exists}
        <span class="ml-2 rounded bg-yellow-100 px-1.5 py-0.5 text-yellow-800 dark:bg-yellow-900/50 dark:text-yellow-200">file not found — showing defaults</span>
      {/if}
    </div>

    <!-- Config sections (AC-1) -->
    <div class="grid gap-4 sm:grid-cols-1 lg:grid-cols-2">
      {#each configView.sections as section (section.name)}
        <div class="rounded-lg border bg-card p-4">
          <div class="mb-3 flex items-center justify-between">
            <h2 class="text-sm font-semibold uppercase tracking-wide text-muted-foreground">{section.name}</h2>
            <span class="text-xs text-muted-foreground">{section.description}</span>
          </div>

          {#if section.redirect_route}
            <!-- AC-2: redirect to dedicated view -->
            <button
              class="flex w-full items-center justify-between rounded-md border border-dashed px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent/50 hover:text-accent-foreground"
              onclick={() => navigateTo(section.redirect_route ?? "")}
            >
              <span>
                {#if section.name === "llm"}
                  See LLM backends
                {:else if section.name === "triggers"}
                  See triggers
                {:else}
                  See details
                {/if}
              </span>
              <span>&rarr;</span>
            </button>
          {:else}
            <!-- Inline key/value display -->
            <div class="space-y-2">
              {#each section.entries as entry (entry.key)}
                <div class="grid grid-cols-2 gap-2">
                  <span class="text-sm text-muted-foreground">{entry.key}</span>
                  <span class="text-sm font-mono">{entry.value}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    <!-- Advanced section (AC-5) -->
    <div class="rounded-lg border bg-card p-4">
      <h2 class="mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground">Advanced</h2>
      <div class="flex items-center justify-between">
        <div>
          <p class="text-sm">Review onboarding</p>
          <p class="text-xs text-muted-foreground">Reset the onboarding flag and restart to see the welcome wizard.</p>
        </div>
        <Button variant="outline" size="sm" onclick={resetOnboarding} disabled={resettingOnboarding}>
          {resettingOnboarding ? "Resetting..." : "Reset Onboarding"}
        </Button>
      </div>
    </div>
  {/if}
</div>
