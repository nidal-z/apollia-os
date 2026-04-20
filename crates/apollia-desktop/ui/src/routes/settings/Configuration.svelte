<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.configuration",
    icon: "sliders",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { navigateTo } from "$lib/stores/navigation";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import { configStore, settingsLoaders } from "$lib/stores/settings";

  let openingEditor = $state(false);
  let openError = $state<string | null>(null);

  async function openEditor() {
    openingEditor = true;
    openError = null;
    try {
      await invoke("open_config_in_editor");
    } catch (err) {
      openError = err instanceof Error ? err.message : String(err);
    } finally {
      openingEditor = false;
    }
  }

  onMount(() => {
    void settingsLoaders.config();
  });
</script>

{#if $configStore.loading && !$configStore.loaded}
  <SettingSectionSkeleton />
{:else}
  <section class="space-y-4" data-testid="runtime-config-section">
    <div class="flex items-center justify-between">
      <p class="text-sm text-muted-foreground">{$t('settings.readonly_banner', { values: { file: 'apollia.toml' } })}</p>
      <Button onclick={openEditor} disabled={openingEditor} variant="outline" size="sm">
        {openingEditor ? $t('settings.opening') : $t('settings.open_editor')}
      </Button>
    </div>

    {#if openError}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">{openError}</div>
    {/if}

    <div class="rounded-md border border-info/20 bg-info/5 px-4 py-3 text-sm text-info-foreground" data-testid="settings-operational-redirect-banner">
      {$t('settings.operational_redirect_banner')}
    </div>

    {#if $configStore.error}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">{$configStore.error}</div>
    {:else if $configStore.data}
      {@const configView = $configStore.data}
      <div class="text-xs text-muted-foreground">
        <span class="font-mono">{configView.config_path}</span>
        {#if !configView.config_exists}
          <span class="ml-2 rounded bg-warning/10 px-1.5 py-0.5 text-warning-foreground">{$t('settings.file_not_found')}</span>
        {/if}
      </div>

      <div class="grid gap-4 sm:grid-cols-1 lg:grid-cols-2">
        {#each configView.sections as section (section.name)}
          <div class="glass-card glass-border rounded-lg p-4">
            <div class="mb-3 flex items-center justify-between">
              <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{section.name}</h3>
              <span class="text-xs text-muted-foreground">{section.description}</span>
            </div>
            {#if section.redirect_route}
              <button
                class="flex w-full items-center justify-between rounded-md border border-dashed border-border px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                onclick={() => navigateTo(section.redirect_route as any)}
                data-testid="settings-redirect-{section.name}"
              >
                <span>
                  {#if section.name === "llm"}
                    {$t('settings.see_llm')}
                  {:else}
                    {$t('settings.see_details')}
                  {/if}
                </span>
                <span>&rarr;</span>
              </button>
            {:else}
              <div class="space-y-2">
                {#each section.entries as entry (entry.key)}
                  <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
                    <span class="text-sm text-muted-foreground">{entry.key}</span>
                    <span class="text-sm font-mono text-foreground">{entry.value}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}
