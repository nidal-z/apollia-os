<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.system",
    icon: "info",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import {
    Info,
    Shield,
    Package,
    Terminal,
    RefreshCw,
    Copy,
    CheckCircle2,
    XCircle,
    Download,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import { ErrorBanner } from "$lib/components/operator";
  import { addToast } from "$lib/components/ui/toast";
  import { humanize, type HumanizedError } from "$lib/errors/humanize";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import SettingsToggle from "../../components/settings/SettingsToggle.svelte";
  import UpdateCheckPanel from "../../components/settings/UpdateCheckPanel.svelte";
  import {
    systemInfoStore,
    securityPostureStore,
    cliStatusStore,
    configStore,
    settingsLoaders,
  } from "$lib/stores/settings";
  import { agentInstallPrefs, setAutoInstallPythonDeps } from "$lib/stores/agentInstallPrefs";
  import { installCli, uninstallCli } from "$lib/ipc/system";
  import type { CliStatus } from "$lib/types";

  let cliActionLoading = $state(false);
  let cliError = $state<HumanizedError | null>(null);
  let refreshing = $state(false);
  let lastRefreshed = $state<Date>(new Date());
  let now = $state<Date>(new Date());
  let tickHandle: ReturnType<typeof setInterval> | undefined;

  async function refreshAll(): Promise<void> {
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

  async function copyPath(path: string | null | undefined): Promise<void> {
    if (!path) return;
    try {
      await navigator.clipboard.writeText(path);
      addToast($t("settings.config.path_copied"), "success");
    } catch {
      addToast($t("settings.config.copy_failed"), "error");
    }
  }

  async function runCli(action: () => Promise<void>): Promise<void> {
    cliActionLoading = true;
    cliError = null;
    try {
      await action();
      await settingsLoaders.cliStatus(true);
    } catch (err) {
      cliError = humanize(err, $t);
    } finally {
      cliActionLoading = false;
    }
  }

  function formatRelative(ref: Date, nowRef: Date): string {
    const seconds = Math.floor(Math.max(0, nowRef.getTime() - ref.getTime()) / 1000);
    if (seconds < 5) return $t("settings.system.refresh_just_now");
    if (seconds < 60)
      return $t("settings.system.refresh_seconds_ago", { values: { n: String(seconds) } });
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60)
      return $t("settings.system.refresh_minutes_ago", { values: { n: String(minutes) } });
    return $t("settings.system.refresh_hours_ago", {
      values: { n: String(Math.floor(minutes / 60)) },
    });
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

{#snippet kvText(label: string, value: string)}
  <div
    class="flex flex-wrap items-center justify-between gap-x-6 gap-y-1 border-b border-border/50 py-2 last:border-b-0"
  >
    <span class="text-body-sm text-muted-foreground">{label}</span>
    <span class="min-w-0 max-w-full truncate font-mono text-code-sm text-foreground">{value}</span>
  </div>
{/snippet}

{#snippet kvPill(label: string, ok: boolean, text: string)}
  <div
    class="flex flex-wrap items-center justify-between gap-x-6 gap-y-1 border-b border-border/50 py-2 last:border-b-0"
  >
    <span class="text-body-sm text-muted-foreground">{label}</span>
    <span class="inline-flex items-center gap-1.5 text-body-sm text-foreground">
      {#if ok}
        <CheckCircle2 size={14} class="text-success" />
      {:else}
        <XCircle size={14} class="text-muted-foreground" />
      {/if}
      {text}
    </span>
  </div>
{/snippet}

{#snippet kvCopyable(label: string, path: string, testid?: string)}
  <div
    class="flex flex-wrap items-center justify-between gap-x-6 gap-y-1 border-b border-border/50 py-2 last:border-b-0"
  >
    <span class="text-body-sm text-muted-foreground">{label}</span>
    <Button
      variant="ghost"
      size="sm"
      class="group min-w-0 max-w-full text-foreground hover:text-primary"
      onclick={() => copyPath(path)}
      title={$t("settings.config.copy_path")}
      data-testid={testid}
    >
      <span class="truncate font-mono text-code-sm">{path}</span>
      <Copy class="ml-1.5 h-3 w-3 shrink-0 opacity-0 transition-opacity group-hover:opacity-100" />
    </Button>
  </div>
{/snippet}

{#if $systemInfoStore.loading && !$systemInfoStore.loaded}
  <SettingSectionSkeleton />
{:else}
  <SettingsSubPage route="system" data-testid="advanced-section">
    <div class="flex items-center justify-between gap-3">
      <span class="text-caption text-muted-foreground" data-testid="system-info-last-refreshed">
        {$t("settings.system.last_refreshed", {
          values: { relative: formatRelative(lastRefreshed, now) },
        })}
      </span>
      <Button
        variant="outline"
        size="sm"
        loading={refreshing}
        onclick={refreshAll}
        data-testid="system-info-refresh-btn"
      >
        {#snippet icon()}<RefreshCw size={13} />{/snippet}
        {$t("settings.system.refresh")}
      </Button>
    </div>

    {#if $systemInfoStore.data}
      {@const systemInfo = $systemInfoStore.data}
      <SettingsSection title={$t("settings.system_info")} data-testid="system-info-section">
        {#snippet icon()}<Info size={15} strokeWidth={1.75} />{/snippet}
        <div>
          {@render kvText($t("settings.system_version"), systemInfo.version)}
          {@render kvText($t("settings.system_os"), systemInfo.os)}
          {#if systemInfo.python_path}
            {@render kvCopyable(
              $t("settings.system_python"),
              systemInfo.python_path,
              "system-info-copy-python",
            )}
          {:else}
            {@render kvText($t("settings.system_python"), $t("settings.system_python_not_found"))}
          {/if}
          {#if $configStore.data?.config_path}
            {@render kvCopyable($t("settings.config.path_label"), $configStore.data.config_path)}
          {/if}
        </div>
      </SettingsSection>
    {:else if $systemInfoStore.error}
      <ErrorBanner message={$systemInfoStore.error} data-testid="system-info-error" />
    {/if}

    {#if $securityPostureStore.data}
      {@const posture = $securityPostureStore.data}
      {@const namespaced = posture.tool_sandbox === "linux_namespaces"}
      <SettingsSection
        title={$t("settings.security.title")}
        data-testid="security-posture-section"
      >
        {#snippet icon()}<Shield size={15} strokeWidth={1.75} />{/snippet}
        {#snippet footer()}
          <p class="text-caption text-muted-foreground">{$t("settings.security.trust_note")}</p>
        {/snippet}
        <div>
          {@render kvText($t("settings.security.platform"), posture.platform)}
          {@render kvPill(
            $t("settings.security.tool_sandbox"),
            namespaced,
            namespaced
              ? $t("settings.security.tool_sandbox_namespaces")
              : $t("settings.security.tool_sandbox_none"),
          )}
          {@render kvPill(
            $t("settings.security.rlimits"),
            posture.rlimits_active,
            posture.rlimits_active
              ? $t("settings.security.rlimits_active")
              : $t("settings.security.rlimits_inactive"),
          )}
          {@render kvText(
            $t("settings.security.agent_execution"),
            $t("settings.security.agent_execution_trusted"),
          )}
        </div>
      </SettingsSection>
    {:else if $securityPostureStore.error}
      <ErrorBanner message={$securityPostureStore.error} data-testid="security-posture-error" />
    {/if}

    <SettingsSection
      title={$t("settings.system.agent_install_section_title")}
      description={$t("settings.system.agent_install_section_desc")}
      data-testid="agent-install-section"
    >
      {#snippet icon()}<Package size={15} strokeWidth={1.75} />{/snippet}
      <SettingsToggle
        id="auto-install-python-deps"
        label={$t("settings.system.auto_install_python_deps_label")}
        description={$t("settings.system.auto_install_python_deps_desc")}
        checked={$agentInstallPrefs.autoInstallPythonDeps}
        onChange={setAutoInstallPythonDeps}
        data-testid="auto-install-python-deps-toggle"
      />
    </SettingsSection>

    <UpdateCheckPanel />

    {#if ($cliStatusStore.data as CliStatus | null)?.bundled}
      {@const cliStatus = $cliStatusStore.data as CliStatus}
      <SettingsSection
        title={$t("settings.cli_title")}
        description={$t("settings.cli_description")}
        data-testid="cli-section"
      >
        {#snippet icon()}<Terminal size={15} strokeWidth={1.75} />{/snippet}
        <div>
          <div
            class="flex flex-wrap items-center justify-between gap-x-6 gap-y-1 border-b border-border/50 py-2"
          >
            <span class="text-body-sm text-muted-foreground">{$t("settings.cli_status")}</span>
            <span class="inline-flex items-center gap-1.5 text-body-sm">
              {#if cliActionLoading}
                <Spinner class="h-3.5 w-3.5 text-muted-foreground" />
                <span class="text-muted-foreground">{$t("settings.system.cli_working")}</span>
              {:else if cliStatus.installed}
                <CheckCircle2 size={14} class="text-success" />
                <span class="font-mono text-code-sm text-foreground"
                  >{$t("settings.cli_installed")} v{cliStatus.version}</span
                >
              {:else}
                <XCircle size={14} class="text-muted-foreground" />
                <span class="text-muted-foreground">{$t("settings.cli_not_installed")}</span>
              {/if}
            </span>
          </div>
          {@render kvCopyable(
            $t("settings.cli_path"),
            cliStatus.symlink_path,
            "system-cli-copy-path",
          )}
        </div>

        <div class="mt-1 flex flex-col gap-1.5">
          {#if cliStatus.installed}
            <div>
              <Button
                variant="outline"
                size="sm"
                loading={cliActionLoading}
                onclick={() => runCli(uninstallCli)}
              >
                {$t("settings.cli_uninstall")}
              </Button>
            </div>
          {:else}
            <div>
              <Button
                variant="default"
                size="sm"
                loading={cliActionLoading}
                onclick={() => runCli(installCli)}
              >
                {#snippet icon()}<Download size={13} />{/snippet}
                {$t("settings.cli_install")}
              </Button>
            </div>
            {#if cliStatus.needs_privilege}
              <p class="text-caption text-muted-foreground">
                {$t("settings.cli_needs_privilege")}
              </p>
            {/if}
          {/if}
          {#if cliError}
            <ErrorBanner tone="danger" data-testid="cli-error">
              <p class="font-medium">{cliError.title}</p>
              <p class="text-caption opacity-90">{cliError.suggested_action}</p>
            </ErrorBanner>
          {/if}
        </div>
      </SettingsSection>
    {/if}
  </SettingsSubPage>
{/if}
