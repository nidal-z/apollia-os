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
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import ConfigFileCard from "../../components/settings/ConfigFileCard.svelte";
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
    <div class="flex items-start justify-between gap-4">
      <p class="text-sm text-muted-foreground">
        {$t("settings.readonly_banner", { values: { file: "apollia.toml" } })}
      </p>
      <Button onclick={openEditor} disabled={openingEditor} variant="outline" size="sm">
        {openingEditor ? $t("settings.opening") : $t("settings.open_editor")}
      </Button>
    </div>

    {#if openError}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
        {openError}
      </div>
    {/if}

    <div class="rounded-md border border-info/20 bg-info/5 px-4 py-3 text-sm text-info-foreground" data-testid="settings-operational-redirect-banner">
      {$t("settings.operational_redirect_banner")}
    </div>

    {#if $configStore.error}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
        {$configStore.error}
      </div>
    {:else if $configStore.data}
      {@const configView = $configStore.data}
      <div class="space-y-3">
        {#each configView.sections as section (section.name)}
          <ConfigFileCard
            {section}
            filePath={configView.config_path}
            fileExists={configView.config_exists}
          />
        {/each}
      </div>
    {/if}
  </section>
{/if}
