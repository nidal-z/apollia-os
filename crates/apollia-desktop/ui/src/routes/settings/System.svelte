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
  import { onMount, onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import { Copy, RefreshCw, Loader2, CheckCircle2, XCircle, Download } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import { addToast } from "$lib/components/ui/toast";
  import { systemInfoStore, cliStatusStore, settingsLoaders } from "$lib/stores/settings";
  import type { CliStatus } from "$lib/types";

  let cliActionLoading = $state(false);
  let cliError = $state<string | null>(null);
  let refreshing = $state(false);
  let lastRefreshed = $state<Date>(new Date());
  let now = $state<Date>(new Date());

  let tickHandle: ReturnType<typeof setInterval> | undefined;

  async function refreshAll() {
    refreshing = true;
    try {
      await Promise.all([settingsLoaders.systemInfo(true), settingsLoaders.cliStatus(true)]);
      lastRefreshed = new Date();
    } finally {
      refreshing = false;
    }
  }

  async function copyPath(path: string | null | undefined) {
    if (!path) return;
    try {
      await navigator.clipboard.writeText(path);
      addToast($t("settings.config.path_copied"), "success");
    } catch {
      addToast($t("settings.config.copy_failed"), "error");
    }
  }

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

  function formatRelative(ref: Date, nowRef: Date): string {
    const diffMs = Math.max(0, nowRef.getTime() - ref.getTime());
    const seconds = Math.floor(diffMs / 1000);
    if (seconds < 5) return $t("settings.system.refresh_just_now");
    if (seconds < 60) return $t("settings.system.refresh_seconds_ago", { values: { n: String(seconds) } });
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return $t("settings.system.refresh_minutes_ago", { values: { n: String(minutes) } });
    const hours = Math.floor(minutes / 60);
    return $t("settings.system.refresh_hours_ago", { values: { n: String(hours) } });
  }

  onMount(() => {
    void settingsLoaders.systemInfo();
    void settingsLoaders.cliStatus();
    tickHandle = setInterval(() => {
      now = new Date();
    }, 15000);
  });

  onDestroy(() => {
    if (tickHandle) clearInterval(tickHandle);
  });
</script>

{#if $systemInfoStore.loading && !$systemInfoStore.loaded}
  <SettingSectionSkeleton />
{:else}
  <section class="space-y-4" data-testid="advanced-section">
    <div class="flex items-center justify-between gap-3">
      <div class="text-xs text-muted-foreground" data-testid="system-info-last-refreshed">
        {$t("settings.system.last_refreshed", {
          values: { relative: formatRelative(lastRefreshed, now) },
        })}
      </div>
      <Button
        variant="outline"
        size="sm"
        onclick={refreshAll}
        disabled={refreshing}
        data-testid="system-info-refresh-btn"
      >
        {#if refreshing}
          <Loader2 class="h-3.5 w-3.5 mr-1.5 animate-spin" />
        {:else}
          <RefreshCw class="h-3.5 w-3.5 mr-1.5" />
        {/if}
        {$t("settings.system.refresh")}
      </Button>
    </div>

    {#if $systemInfoStore.data}
      {@const systemInfo = $systemInfoStore.data}
      <div class="glass-card glass-border rounded-lg p-4" data-testid="system-info-section">
        <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {$t("settings.system_info")}
        </h3>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-2">
          <div class="grid grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t("settings.system_version")}</span>
            <span class="text-sm font-mono text-foreground">{systemInfo.version}</span>
          </div>
          <div class="grid grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t("settings.system_os")}</span>
            <span class="text-sm font-mono text-foreground">{systemInfo.os}</span>
          </div>
          <div class="grid grid-cols-2 gap-2 sm:col-span-2">
            <span class="text-sm text-muted-foreground">{$t("settings.system_python")}</span>
            {#if systemInfo.python_path}
              <button
                type="button"
                class="group inline-flex items-center gap-1.5 text-left text-sm font-mono text-foreground hover:text-primary"
                onclick={() => copyPath(systemInfo.python_path)}
                title={$t("settings.config.copy_path")}
                data-testid="system-info-copy-python"
              >
                <span class="truncate">{systemInfo.python_path}</span>
                <Copy class="h-3 w-3 opacity-0 group-hover:opacity-100 shrink-0" />
              </button>
            {:else}
              <span class="text-sm font-mono text-muted-foreground italic">{$t("settings.system_python_not_found")}</span>
            {/if}
          </div>
        </div>
      </div>
    {:else if $systemInfoStore.error}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
        {$systemInfoStore.error}
      </div>
    {/if}

    {#if ($cliStatusStore.data as CliStatus | null)?.bundled}
      {@const cliStatus = $cliStatusStore.data as CliStatus}
      <div class="glass-card glass-border rounded-lg p-4">
        <h3 class="mb-1 text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {$t("settings.cli_title")}
        </h3>
        <p class="text-sm text-muted-foreground mb-3">{$t("settings.cli_description")}</p>
        <div class="space-y-2 mb-3">
          <div class="grid grid-cols-2 gap-2 items-center">
            <span class="text-sm text-muted-foreground">{$t("settings.cli_status")}</span>
            <span class="inline-flex items-center gap-1.5 text-sm">
              {#if cliActionLoading}
                <Loader2 class="h-3.5 w-3.5 animate-spin text-muted-foreground" />
                <span class="text-muted-foreground">{$t("settings.system.cli_working")}</span>
              {:else if cliStatus.installed}
                <CheckCircle2 class="h-3.5 w-3.5 text-success" />
                <span class="font-mono">{$t("settings.cli_installed")} v{cliStatus.version}</span>
              {:else}
                <XCircle class="h-3.5 w-3.5 text-muted-foreground" />
                <span class="text-muted-foreground">{$t("settings.cli_not_installed")}</span>
              {/if}
            </span>
          </div>
          <div class="grid grid-cols-2 gap-2 items-center">
            <span class="text-sm text-muted-foreground">{$t("settings.cli_path")}</span>
            <button
              type="button"
              class="group inline-flex items-center gap-1.5 text-left text-sm font-mono text-foreground hover:text-primary"
              onclick={() => copyPath(cliStatus.symlink_path)}
              title={$t("settings.config.copy_path")}
              data-testid="system-cli-copy-path"
            >
              <span class="truncate">{cliStatus.symlink_path}</span>
              <Copy class="h-3 w-3 opacity-0 group-hover:opacity-100 shrink-0" />
            </button>
          </div>
        </div>
        {#if cliStatus.installed}
          <Button variant="outline" size="sm" onclick={uninstallCli} disabled={cliActionLoading}>
            {cliActionLoading ? $t("common.loading") : $t("settings.cli_uninstall")}
          </Button>
        {:else}
          <Button variant="default" size="sm" onclick={installCli} disabled={cliActionLoading}>
            {#if cliActionLoading}
              <Loader2 class="h-3.5 w-3.5 mr-1.5 animate-spin" />
            {:else}
              <Download class="h-3.5 w-3.5 mr-1.5" />
            {/if}
            {cliActionLoading ? $t("common.loading") : $t("settings.cli_install")}
          </Button>
          {#if cliStatus.needs_privilege}
            <p class="text-xs text-muted-foreground mt-1">{$t("settings.cli_needs_privilege")}</p>
          {/if}
        {/if}
        {#if cliError}
          <p class="text-sm text-destructive mt-2">{cliError}</p>
        {/if}
      </div>
    {/if}
  </section>
{/if}
