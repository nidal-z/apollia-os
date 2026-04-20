<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.system",
    icon: "info",
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
  import { systemInfoStore, cliStatusStore, settingsLoaders } from "$lib/stores/settings";
  import type { CliStatus } from "$lib/types";

  let cliActionLoading = $state(false);
  let cliError = $state<string | null>(null);

  async function installCli() {
    cliActionLoading = true;
    cliError = null;
    try {
      await invoke("install_cli");
      await settingsLoaders.cliStatus(true);
    } catch (err) {
      cliError = err instanceof Error ? err.message : String(err);
    } finally {
      cliActionLoading = false;
    }
  }

  async function uninstallCli() {
    cliActionLoading = true;
    cliError = null;
    try {
      await invoke("uninstall_cli");
      await settingsLoaders.cliStatus(true);
    } catch (err) {
      cliError = err instanceof Error ? err.message : String(err);
    } finally {
      cliActionLoading = false;
    }
  }

  onMount(() => {
    void settingsLoaders.systemInfo();
    void settingsLoaders.cliStatus();
  });
</script>

{#if $systemInfoStore.loading && !$systemInfoStore.loaded}
  <SettingSectionSkeleton />
{:else}
  <section class="space-y-4" data-testid="advanced-section">
    {#if $systemInfoStore.data}
      {@const systemInfo = $systemInfoStore.data}
      <div class="glass-card glass-border rounded-lg p-4" data-testid="system-info-section">
        <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.system_info')}</h3>
        <div class="space-y-2">
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t('settings.system_version')}</span>
            <span class="text-sm font-mono text-foreground">{systemInfo.version}</span>
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t('settings.system_os')}</span>
            <span class="text-sm font-mono text-foreground">{systemInfo.os}</span>
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t('settings.system_python')}</span>
            <span class="text-sm font-mono text-foreground">{systemInfo.python_path ?? $t('settings.system_python_not_found')}</span>
          </div>
        </div>
      </div>
    {:else if $systemInfoStore.error}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">{$systemInfoStore.error}</div>
    {/if}

    {#if ($cliStatusStore.data as CliStatus | null)?.bundled}
      {@const cliStatus = $cliStatusStore.data as CliStatus}
      <div class="glass-card glass-border rounded-lg p-4">
        <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.cli_title')}</h3>
        <p class="text-sm text-muted-foreground mb-3">{$t('settings.cli_description')}</p>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 mb-3">
          <span class="text-sm text-muted-foreground">{$t('settings.cli_status')}</span>
          <span class="text-sm font-mono text-foreground">
            {cliStatus.installed ? $t('settings.cli_installed') : $t('settings.cli_not_installed')}
          </span>
          <span class="text-sm text-muted-foreground">{$t('settings.cli_path')}</span>
          <span class="text-sm font-mono text-foreground">{cliStatus.symlink_path}</span>
        </div>
        {#if cliStatus.installed}
          <Button variant="outline" size="sm" onclick={uninstallCli} disabled={cliActionLoading}>
            {cliActionLoading ? $t('common.loading') : $t('settings.cli_uninstall')}
          </Button>
        {:else}
          <Button variant="default" size="sm" onclick={installCli} disabled={cliActionLoading}>
            {cliActionLoading ? $t('common.loading') : $t('settings.cli_install')}
          </Button>
          {#if cliStatus.needs_privilege}
            <p class="text-xs text-muted-foreground mt-1">{$t('settings.cli_needs_privilege')}</p>
          {/if}
        {/if}
        {#if cliError}
          <p class="text-sm text-destructive mt-2">{cliError}</p>
        {/if}
      </div>
    {/if}
  </section>
{/if}
