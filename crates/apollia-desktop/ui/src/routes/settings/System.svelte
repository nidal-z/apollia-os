<script lang="ts" context="module">
  import { Card } from "$lib/components/ui/card";
  import { Spinner } from "$lib/components/ui/progress";
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
  import { Copy, RefreshCw, CheckCircle2, XCircle, Download } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import UpdateChecker from "./UpdateChecker.svelte";
  import { addToast } from "$lib/components/ui/toast";
  import { systemInfoStore, securityPostureStore, cliStatusStore, configStore, settingsLoaders } from "$lib/stores/settings";
  import { agentInstallPrefs, setAutoInstallPythonDeps } from "$lib/stores/agentInstallPrefs";
  import SettingsToggle from "../../components/settings/SettingsToggle.svelte";
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
      await Promise.all([
        settingsLoaders.systemInfo(true),
        settingsLoaders.securityPosture(true),
        settingsLoaders.cliStatus(true),
      ]);
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
    void settingsLoaders.securityPosture();
    void settingsLoaders.cliStatus();
    void settingsLoaders.config();
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
          <Spinner class="h-3.5 w-3.5 mr-1.5" />
        {:else}
          <RefreshCw class="h-3.5 w-3.5 mr-1.5" />
        {/if}
        {$t("settings.system.refresh")}
      </Button>
    </div>

    {#if $systemInfoStore.data}
      {@const systemInfo = $systemInfoStore.data}
      <Card class="rounded-lg p-4" data-testid="system-info-section">
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
              <Button variant="ghost" size="sm"
                type="button"
                class="group inline-flex items-center gap-1.5 text-left text-sm font-mono text-foreground hover:text-primary"
                onclick={() => copyPath(systemInfo.python_path)}
                title={$t("settings.config.copy_path")}
                data-testid="system-info-copy-python"
              >
                <span class="truncate">{systemInfo.python_path}</span>
                <Copy class="h-3 w-3 opacity-0 group-hover:opacity-100 shrink-0" />
              </Button>
            {:else}
              <span class="text-sm font-mono text-muted-foreground italic">{$t("settings.system_python_not_found")}</span>
            {/if}
          </div>
          {#if $configStore.data?.config_path}
            <div class="grid grid-cols-2 gap-2 sm:col-span-2">
              <span class="text-sm text-muted-foreground">{$t("settings.config.path_label")}</span>
              <Button variant="ghost" size="sm"
                type="button"
                class="group inline-flex items-center gap-1.5 text-left text-sm font-mono text-foreground hover:text-primary"
                onclick={() => copyPath($configStore.data?.config_path)}
                title={$t("settings.config.copy_path")}
              >
                <span class="truncate">{$configStore.data.config_path}</span>
                <Copy class="h-3 w-3 opacity-0 group-hover:opacity-100 shrink-0" />
              </Button>
            </div>
          {/if}
        </div>
      </Card>
    {:else if $systemInfoStore.error}
      <ErrorBanner message={$systemInfoStore.error} />
    {/if}

    {#if $securityPostureStore.data}
      {@const posture = $securityPostureStore.data}
      {@const namespaced = posture.tool_sandbox === "linux_namespaces"}
      <Card class="rounded-lg p-4" data-testid="security-posture-section">
        <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {$t("settings.security.title")}
        </h3>
        <div class="grid grid-cols-1 gap-y-2">
          <div class="grid grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t("settings.security.platform")}</span>
            <span class="text-sm font-mono text-foreground">{posture.platform}</span>
          </div>
          <div class="grid grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t("settings.security.tool_sandbox")}</span>
            <span class="inline-flex items-center gap-1.5 text-sm text-foreground">
              {#if namespaced}
                <CheckCircle2 class="h-3.5 w-3.5 text-success" />
                {$t("settings.security.tool_sandbox_namespaces")}
              {:else}
                <XCircle class="h-3.5 w-3.5 text-muted-foreground" />
                {$t("settings.security.tool_sandbox_none")}
              {/if}
            </span>
          </div>
          <div class="grid grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t("settings.security.rlimits")}</span>
            <span class="inline-flex items-center gap-1.5 text-sm text-foreground">
              {#if posture.rlimits_active}
                <CheckCircle2 class="h-3.5 w-3.5 text-success" />
                {$t("settings.security.rlimits_active")}
              {:else}
                <XCircle class="h-3.5 w-3.5 text-muted-foreground" />
                {$t("settings.security.rlimits_inactive")}
              {/if}
            </span>
          </div>
          <div class="grid grid-cols-2 gap-2">
            <span class="text-sm text-muted-foreground">{$t("settings.security.agent_execution")}</span>
            <span class="text-sm text-foreground">{$t("settings.security.agent_execution_trusted")}</span>
          </div>
        </div>
        <p class="mt-3 text-xs text-muted-foreground">
          {$t("settings.security.trust_note")}
        </p>
      </Card>
    {:else if $securityPostureStore.error}
      <ErrorBanner message={$securityPostureStore.error} />
    {/if}

    <Card class="rounded-lg p-4" data-testid="agent-install-section">
      <h3 class="mb-1 text-sm font-medium uppercase tracking-wider text-muted-foreground">
        {$t("settings.system.agent_install_section_title")}
      </h3>
      <p class="mb-3 text-xs text-muted-foreground leading-relaxed">
        {$t("settings.system.agent_install_section_desc")}
      </p>
      <SettingsToggle
        id="auto-install-python-deps"
        label={$t("settings.system.auto_install_python_deps_label")}
        description={$t("settings.system.auto_install_python_deps_desc")}
        checked={$agentInstallPrefs.autoInstallPythonDeps}
        onChange={setAutoInstallPythonDeps}
        data-testid="auto-install-python-deps-toggle"
      />
    </Card>

    <UpdateChecker />

    {#if ($cliStatusStore.data as CliStatus | null)?.bundled}
      {@const cliStatus = $cliStatusStore.data as CliStatus}
      <Card class="rounded-lg p-4">
        <h3 class="mb-1 text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {$t("settings.cli_title")}
        </h3>
        <p class="text-sm text-muted-foreground mb-3">{$t("settings.cli_description")}</p>
        <div class="space-y-2 mb-3">
          <div class="grid grid-cols-2 gap-2 items-center">
            <span class="text-sm text-muted-foreground">{$t("settings.cli_status")}</span>
            <span class="inline-flex items-center gap-1.5 text-sm">
              {#if cliActionLoading}
                <Spinner class="h-3.5 w-3.5 text-muted-foreground" />
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
            <Button variant="ghost" size="sm"
              type="button"
              class="group inline-flex items-center gap-1.5 text-left text-sm font-mono text-foreground hover:text-primary"
              onclick={() => copyPath(cliStatus.symlink_path)}
              title={$t("settings.config.copy_path")}
              data-testid="system-cli-copy-path"
            >
              <span class="truncate">{cliStatus.symlink_path}</span>
              <Copy class="h-3 w-3 opacity-0 group-hover:opacity-100 shrink-0" />
            </Button>
          </div>
        </div>
        {#if cliStatus.installed}
          <Button variant="outline" size="sm" onclick={uninstallCli} disabled={cliActionLoading}>
            {cliActionLoading ? $t("common.loading") : $t("settings.cli_uninstall")}
          </Button>
        {:else}
          <Button variant="default" size="sm" onclick={installCli} disabled={cliActionLoading}>
            {#if cliActionLoading}
              <Spinner class="h-3.5 w-3.5 mr-1.5" />
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
      </Card>
    {/if}
  </section>
{/if}
