<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { Card } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Play, Square } from "lucide-svelte";
  interface Props {
    agent: AgentListItem;
    onlogs: (agentId: string) => void;
    ondetail: (agent: AgentListItem) => void;
  }

  let { agent, onlogs, ondetail }: Props = $props();

  type RuntimeState = "active" | "degraded" | "stopped" | "initializing" | "stopping";

  const STATUS_CONFIG: Record<
    RuntimeState,
    { labelKey: string; variant: "default" | "secondary" | "destructive" | "outline" | "success" | "warning"; extraClass: string }
  > = {
    active: { labelKey: "common.status.active", variant: "success", extraClass: "" },
    degraded: { labelKey: "common.status.degraded", variant: "warning", extraClass: "" },
    stopped: { labelKey: "common.status.stopped", variant: "secondary", extraClass: "" },
    initializing: { labelKey: "common.status.initializing", variant: "outline", extraClass: "animate-pulse border-info text-info" },
    stopping: { labelKey: "common.status.stopping", variant: "outline", extraClass: "border-warning text-warning" },
  };

  let stopping = $state(false);
  let confirmVisible = $state(false);
  let stopError = $state<string | null>(null);
  let toggleLoading = $state(false);
  let uninstallLoading = $state(false);
  let updateLoading = $state(false);
  let installLoading = $state(false);
  let startLoading = $state(false);
  let actionError = $state<string | null>(null);

  function agentColor(name: string): string {
    const hash = name.split("").reduce((acc, char) => acc + char.charCodeAt(0), 0);
    const hue = hash % 360;
    return `hsl(${hue}, 60%, 60%)`;
  }

  function agentInitial(name: string): string {
    return name.charAt(0).toUpperCase();
  }

  const isInstalled = $derived(agent.installed_at !== null);
  const runtimeStatus = $derived(agent.runtime_status as RuntimeState | null);
  const isRunning = $derived(runtimeStatus === "active" || runtimeStatus === "degraded");
  const isLoaded = $derived(runtimeStatus !== null);
  const config = $derived(runtimeStatus ? STATUS_CONFIG[runtimeStatus] : null);

  async function handleStop() {
    if (!agent.id) return;
    confirmVisible = false;
    stopping = true;
    stopError = null;
    try {
      await invoke("stop_agent", { agentId: agent.id });
    } catch (err: unknown) {
      stopError = err instanceof Error ? err.message : String(err);
    } finally {
      stopping = false;
    }
  }

  function handleStopClick() {
    confirmVisible = true;
  }

  function handleCancelStop() {
    confirmVisible = false;
  }

  function handleLogsClick() {
    if (agent.id) {
      onlogs(agent.id);
    }
  }

  async function handleStart() {
    if (!agent.install_path) return;
    startLoading = true;
    actionError = null;
    try {
      await invoke("start_agent", { path: agent.install_path });
    } catch (err: unknown) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      startLoading = false;
    }
  }

  async function handleToggleEnabled() {
    toggleLoading = true;
    actionError = null;
    try {
      if (agent.enabled) {
        await invoke("disable_agent", { name: agent.name });
      } else {
        await invoke("enable_agent", { name: agent.name });
      }
    } catch (err: unknown) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      toggleLoading = false;
    }
  }

  async function handleUninstall() {
    const confirmed = await confirm(
      $t("agents.uninstall_confirm_message", { values: { name: agent.name } }),
      { title: $t("agents.uninstall_confirm_title"), kind: "warning" },
    );
    if (!confirmed) return;

    uninstallLoading = true;
    actionError = null;
    try {
      await invoke("uninstall_agent", { name: agent.name });
    } catch (err: unknown) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      uninstallLoading = false;
    }
  }

  async function handleUpdate() {
    const path = await openDialog({
      filters: [{ name: "Python Agent", extensions: ["py"] }],
      multiple: false,
    });
    if (!path) return;

    updateLoading = true;
    actionError = null;
    try {
      await invoke("update_agent", { name: agent.name, path });
    } catch (err: unknown) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      updateLoading = false;
    }
  }

  async function handleInstallPermanently() {
    installLoading = true;
    actionError = null;
    try {
      await invoke("install_agent", { path: agent.name });
    } catch (err: unknown) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      installLoading = false;
    }
  }
</script>

<Card
  class="relative cursor-pointer overflow-hidden transition-all duration-200 hover:bg-[rgba(52,53,245,0.04)] dark:hover:bg-[rgba(124,95,214,0.06)] hover:shadow-md"
  data-testid="agent-card"
  data-agent-name={agent.name}
  data-agent-state={agent.runtime_status ?? "not-loaded"}
  onclick={() => ondetail(agent)}
>
  <!-- Avatar + Name + Version + Badge -->
  <div class="px-4 pt-4 pb-3">
    <div class="flex items-start gap-3">
      <!-- Avatar -->
      <div
        class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-sm font-semibold text-white"
        style="background-color: {agentColor(agent.name)}"
        data-testid="agent-avatar"
      >
        {agentInitial(agent.name)}
      </div>

      <!-- Name + Badge -->
      <div class="min-w-0 flex-1">
        <div class="flex items-center justify-between gap-2">
          <h3 class="truncate text-base font-semibold" data-testid="agent-name">
            {agent.name}
          </h3>
          <div class="flex shrink-0 items-center gap-1.5">
            {#if !isInstalled}
              <Badge variant="outline" class="whitespace-nowrap text-[10px] px-1.5 py-0" data-testid="agent-session-badge">
                {$t("agents.session_only")}
              </Badge>
            {/if}
            {#if config}
              <Badge variant={config.variant} class="whitespace-nowrap text-[10px] px-1.5 py-0 {config.extraClass}" data-testid="agent-status">
                {$t(config.labelKey)}
              </Badge>
            {:else}
              <Badge variant="secondary" class="whitespace-nowrap text-[10px] px-1.5 py-0" data-testid="agent-status">
                {$t("agents.not_loaded")}
              </Badge>
            {/if}
          </div>
        </div>

        <!-- Version + description -->
        <p class="mt-0.5 text-xs text-muted-foreground" data-testid="agent-version">
          v{agent.version}
        </p>
        {#if agent.description}
          <p class="mt-1 line-clamp-2 text-xs text-muted-foreground/80" data-testid="agent-description">
            {agent.description}
          </p>
        {/if}
      </div>
    </div>

    <!-- Tags -->
    {#if agent.tags.length > 0}
      <div class="mt-2 flex flex-wrap gap-1">
        {#each agent.tags as tag}
          <span class="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground">{tag}</span>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Enabled / Disabled toggle + Start button for installed agents -->
  {#if isInstalled}
    <div class="border-t px-4 py-2.5">
      <div class="flex items-center justify-between">
        <span class="text-xs text-muted-foreground">
          {#if agent.enabled}
            {$t("agents.auto_start_enabled")}
          {:else}
            {$t("agents.auto_start_disabled")}
          {/if}
        </span>
        <div class="flex items-center gap-2">
          <!-- Start button for installed but not loaded agents -->
          {#if !isLoaded && agent.install_path}
            <Button
              size="sm"
              variant="outline"
              class="h-6 gap-1 px-2 text-xs"
              onclick={(e: MouseEvent) => { e.stopPropagation(); handleStart(); }}
              disabled={startLoading}
              title={$t("agents.start_tooltip")}
              data-testid="agent-start-btn"
            >
              <Play size={12} />
              {startLoading ? $t("agents.starting_agent") : $t("agents.start")}
            </Button>
          {/if}
          <!-- Toggle -->
          <button
            class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {agent.enabled ? 'bg-primary' : 'bg-muted-foreground/30'}"
            onclick={(e) => { e.stopPropagation(); handleToggleEnabled(); }}
            disabled={toggleLoading}
            title={agent.enabled ? $t("agents.disable_tooltip") : $t("agents.enable_tooltip")}
            data-testid="agent-enabled-toggle"
          >
            <span
              class="inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform {agent.enabled ? 'translate-x-[18px]' : 'translate-x-[3px]'}"
            ></span>
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Error display -->
  {#if stopError || actionError}
    <div class="border-t px-4 py-2">
      <p class="text-xs text-[hsl(var(--destructive))]">{stopError || actionError}</p>
    </div>
  {/if}

  <!-- Actions -->
  <div class="border-t px-4 py-2" onclick={(e) => e.stopPropagation()}>
    {#if confirmVisible}
      <div class="flex flex-wrap items-center gap-1">
        <span class="text-xs text-muted-foreground">{$t("agents.stop_confirm")}</span>
        <Button size="sm" variant="destructive" class="h-7 px-2 text-xs" onclick={handleStop} disabled={stopping} data-testid="agent-stop-confirm-btn">
          {stopping ? $t("agents.stopping") : $t("common.confirm")}
        </Button>
        <Button size="sm" variant="outline" class="h-7 px-2 text-xs" onclick={handleCancelStop}>
          {$t("common.cancel")}
        </Button>
      </div>
    {:else}
      <div class="flex flex-wrap items-center gap-1">
        {#if isRunning && agent.id}
          <Button size="sm" variant="outline" class="h-7 px-2 text-xs" onclick={handleStopClick} data-testid="agent-stop-btn">
            <Square size={10} class="mr-1" />
            {$t("agents.stop")}
          </Button>
        {/if}
        {#if agent.id}
          <Button size="sm" variant="ghost" class="h-7 px-2 text-xs" onclick={handleLogsClick} data-testid="agent-logs-btn">
            {$t("agents.logs")}
          </Button>
        {/if}
        {#if isInstalled}
          <Button size="sm" variant="ghost" class="h-7 px-2 text-xs" onclick={handleUpdate} disabled={updateLoading} data-testid="agent-update-button">
            {updateLoading ? $t("common.loading") : $t("agents.update")}
          </Button>
          <Button size="sm" variant="ghost" class="h-7 px-2 text-xs text-[hsl(var(--destructive))]" onclick={handleUninstall} disabled={uninstallLoading} data-testid="agent-uninstall-button">
            {uninstallLoading ? $t("common.loading") : $t("agents.uninstall")}
          </Button>
        {:else}
          <Button size="sm" variant="outline" class="h-7 px-2 text-xs" onclick={handleInstallPermanently} disabled={installLoading} data-testid="install-permanently-btn">
            {installLoading ? $t("common.loading") : $t("agents.install_permanently")}
          </Button>
        {/if}
      </div>
    {/if}
  </div>
</Card>
