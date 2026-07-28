<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.configuration",
    icon: "sliders",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { ExternalLink, FileText } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import { humanize } from "$lib/errors/humanize";
  import { reportError } from "$lib/errors/reportError";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import ConfigFileCard from "../../components/settings/ConfigFileCard.svelte";
  import { configStore, settingsLoaders } from "$lib/stores/settings";
  import { openConfigInEditor } from "$lib/ipc/app";

  let openingEditor = $state(false);

  // Read-only page: no explicit-save footer (onSave omitted on the frame).
  // The config file is edited externally, then the runtime is restarted.
  const readError = $derived(
    $configStore.error ? humanize($configStore.error, $t) : null,
  );

  async function openEditor(): Promise<void> {
    openingEditor = true;
    try {
      await openConfigInEditor();
    } catch (err) {
      reportError(err, {
        surface: "toast",
        testid: "config-open-editor-error-toast",
      });
    } finally {
      openingEditor = false;
    }
  }

  function reload(): void {
    void settingsLoaders.config();
  }

  onMount(reload);
</script>

{#if $configStore.loading && !$configStore.loaded}
  <SettingSectionSkeleton />
{:else}
  <SettingsSubPage route="configuration" data-testid="configuration-subpage">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <p class="max-w-prose text-body-sm text-muted-foreground">
        {$t("settings.readonly_banner", { values: { file: "apollia.toml" } })}
      </p>
      <Button onclick={openEditor} disabled={openingEditor} variant="outline" size="sm">
        {#snippet icon()}<ExternalLink size={13} />{/snippet}
        {openingEditor ? $t("settings.opening") : $t("settings.open_editor")}
      </Button>
    </div>

    <ErrorBanner tone="info" data-testid="settings-operational-redirect-banner">
      {$t("settings.operational_redirect_banner")}
    </ErrorBanner>

    {#if readError}
      <ErrorBanner
        tone="danger"
        onretry={reload}
        retryLabel={$t("common.retry")}
        loading={$configStore.loading}
        data-testid="config-read-error"
      >
        <p class="font-medium">{readError.title}</p>
        <p class="text-caption opacity-90">{readError.suggested_action}</p>
      </ErrorBanner>
    {:else if $configStore.data}
      {@const view = $configStore.data}
      {#if view.sections.length === 0}
        <div
          class="flex flex-col items-center gap-2.5 rounded-xl border border-border bg-card px-6 py-10 text-center"
          data-testid="config-empty"
        >
          <span
            class="flex h-10 w-10 items-center justify-center rounded-lg bg-muted/50 text-muted-foreground"
          >
            <FileText size={20} />
          </span>
          <p class="text-body-sm font-medium text-foreground">
            {$t("settings.config.empty_title")}
          </p>
          <p class="max-w-sm text-caption text-muted-foreground">
            {$t("settings.config.empty")}
          </p>
        </div>
      {:else}
        <div class="space-y-3">
          {#each view.sections as section, i (section.name)}
            <ConfigFileCard
              {section}
              filePath={view.config_path}
              fileExists={view.config_exists}
              defaultExpanded={i === 0}
            />
          {/each}
        </div>
      {/if}
    {/if}
  </SettingsSubPage>
{/if}
