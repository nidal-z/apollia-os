<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Play, Square, MessageSquare, Trash2, RefreshCw } from "lucide-svelte";

  interface Props {
    agent: AgentListItem;
    onlogs: (agentId: string) => void;
    ondetail: (agent: AgentListItem) => void;
    onchat?: (agentName: string) => void;
  }

  let { agent, onlogs, ondetail, onchat }: Props = $props();

  type RuntimeState = "active" | "degraded" | "stopped" | "initializing" | "stopping";

  const STATUS_CONFIG: Record<
    RuntimeState,
    { labelKey: string; variant: "success" | "warning" | "secondary" | "info" | "outline" }
  > = {
    active: { labelKey: "common.status.active", variant: "success" },
    degraded: { labelKey: "common.status.degraded", variant: "warning" },
    stopped: { labelKey: "common.status.stopped", variant: "secondary" },
    initializing: { labelKey: "common.status.initializing", variant: "info" },
    stopping: { labelKey: "common.status.stopping", variant: "warning" },
  };

  let stopping = $state(false);
  let confirmVisible = $state(false);
  let uninstallConfirmVisible = $state(false);
  let deleteMemoryData = $state(false);
  let stopError = $state<string | null>(null);
  let toggleLoading = $state(false);
  let uninstallLoading = $state(false);
  let updateLoading = $state(false);
  let installLoading = $state(false);
  let startLoading = $state(false);
  let actionError = $state<string | null>(null);

  function avatarHue(name: string): number {
    return name.split("").reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360;
  }

  const hue = $derived(avatarHue(agent.name));
  const isInstalled = $derived(agent.installed_at !== null);
  const runtimeStatus = $derived(agent.runtime_status as RuntimeState | null);
  const isRunning = $derived(runtimeStatus === "active" || runtimeStatus === "degraded");
  const isLoaded = $derived(runtimeStatus !== null);
  const config = $derived(runtimeStatus ? STATUS_CONFIG[runtimeStatus] : null);

  async function handleStop() {
    if (!agent.id) return;
    confirmVisible = false; stopping = true; stopError = null;
    try { await invoke("stop_agent", { agentId: agent.id }); }
    catch (err: unknown) { stopError = err instanceof Error ? err.message : String(err); }
    finally { stopping = false; }
  }

  async function handleStart() {
    if (!agent.install_path) return;
    startLoading = true; actionError = null;
    try { await invoke("start_agent", { path: agent.install_path }); }
    catch (err: unknown) { actionError = err instanceof Error ? err.message : String(err); }
    finally { startLoading = false; }
  }

  async function handleToggleEnabled() {
    toggleLoading = true; actionError = null;
    try {
      if (agent.enabled) { await invoke("disable_agent", { name: agent.name }); }
      else { await invoke("enable_agent", { name: agent.name }); }
    }
    catch (err: unknown) { actionError = err instanceof Error ? err.message : String(err); }
    finally { toggleLoading = false; }
  }

  function handleUninstall() {
    confirmVisible = false;
    deleteMemoryData = false;
    uninstallConfirmVisible = true;
  }

  async function confirmUninstall() {
    uninstallConfirmVisible = false;
    uninstallLoading = true;
    actionError = null;
    try {
      // Stop from runtime first so it's removed from the in-memory registry.
      // Without this, list_agents returns it as a "session-only" ghost entry.
      if (agent.id) {
        try { await invoke("stop_agent", { agentId: agent.id }); } catch { /* already stopped */ }
      }

      // Best-effort: delete agent memory entries before removing the agent.
      if (deleteMemoryData) {
        try {
          const entries = await invoke<{ id: string }[]>("list_memory_entries", { namespace: agent.name });
          for (const entry of entries) {
            await invoke("delete_memory_entry", { namespace: agent.name, entryId: entry.id });
          }
        } catch { /* memory namespace may not exist — not critical */ }
      }

      await invoke("uninstall_agent", { name: agent.name });

      // Optimistic update: remove immediately so the card doesn't linger.
      agents.update((list) => list.filter((a) => a.name !== agent.name));
    } catch (err: unknown) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      uninstallLoading = false;
    }
  }

  async function handleUpdate() {
    const path = await openDialog({ filters: [{ name: "Python Agent", extensions: ["py"] }], multiple: false });
    if (!path) return;
    updateLoading = true; actionError = null;
    try { await invoke("update_agent", { name: agent.name, path }); }
    catch (err: unknown) { actionError = err instanceof Error ? err.message : String(err); }
    finally { updateLoading = false; }
  }

  async function handleInstallPermanently() {
    installLoading = true; actionError = null;
    try { await invoke("install_agent", { path: agent.name }); }
    catch (err: unknown) { actionError = err instanceof Error ? err.message : String(err); }
    finally { installLoading = false; }
  }
</script>

<div
  class="glass-card-hover glass-border rounded-lg overflow-hidden h-full flex flex-col cursor-pointer"
  data-testid="agent-card"
  data-agent-name={agent.name}
  data-agent-state={agent.runtime_status ?? "not-loaded"}
  onclick={() => ondetail(agent)}
  onkeydown={(e) => { if (e.key === "Enter") ondetail(agent); }}
  role="button"
  tabindex="0"
>
  <!-- Status accent bar -->
  <div class="h-0.5 w-full {isRunning ? 'bg-primary' : runtimeStatus === 'stopped' ? 'bg-muted-foreground/20' : 'bg-muted'}"></div>

  <!-- Main content -->
  <div class="px-3.5 pt-3 pb-2.5 flex-1 flex flex-col">
    <!-- Row 1: avatar + name + badge -->
    <div class="flex items-center gap-2.5">
      <div
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-xs font-semibold text-white"
        style="background: hsl({hue}, 60%, 48%); box-shadow: 0 2px 8px -1px hsla({hue}, 60%, 38%, 0.3);"
        data-testid="agent-avatar"
      >
        {agent.name.charAt(0).toUpperCase()}
      </div>
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="truncate text-[13px] font-medium" data-testid="agent-name">{agent.name}</span>
          <span class="shrink-0 text-[10px] text-muted-foreground/50" data-testid="agent-version">v{agent.version}</span>
        </div>
      </div>
      <div class="flex items-center gap-1">
        {#if !isInstalled}
          <Badge variant="outline" class="text-[9px] px-1.5 py-0" data-testid="agent-session-badge">{$t("agents.session_only")}</Badge>
        {/if}
        {#if config}
          <Badge variant={config.variant} class="text-[9px] px-1.5 py-0" data-testid="agent-status">{$t(config.labelKey)}</Badge>
        {:else}
          <Badge variant="secondary" class="text-[9px] px-1.5 py-0" data-testid="agent-status">{$t("agents.not_loaded")}</Badge>
        {/if}
      </div>
    </div>

    <!-- Description -->
    <p class="mt-2 line-clamp-2 text-xs text-muted-foreground leading-relaxed flex-1" data-testid="agent-description">
      {agent.description ?? ""}
    </p>

    <!-- Tags -->
    {#if agent.tags.length > 0}
      <div class="mt-2 flex flex-wrap gap-1">
        {#each agent.tags.slice(0, 4) as tag}
          <span class="rounded bg-muted/60 px-1.5 py-px text-[10px] text-muted-foreground">{tag}</span>
        {/each}
        {#if agent.tags.length > 4}
          <span class="text-[10px] text-muted-foreground/40">+{agent.tags.length - 4}</span>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Auto-start toggle (installed agents only) -->
  {#if isInstalled}
    <div class="border-t border-border/40 px-3.5 py-2 flex items-center justify-between" onclick={(e) => e.stopPropagation()}>
      <span class="text-[11px] text-muted-foreground">
        {agent.enabled ? $t("agents.auto_start_enabled") : $t("agents.auto_start_disabled")}
      </span>
      <Toggle checked={agent.enabled} onchange={handleToggleEnabled} disabled={toggleLoading} size="sm" aria-label={agent.enabled ? $t("agents.auto_start_enabled") : $t("agents.auto_start_disabled")} data-testid="agent-enabled-toggle" />
    </div>
  {/if}

  <!-- Error -->
  {#if stopError || actionError}
    <div class="border-t border-destructive/20 px-3.5 py-1.5">
      <p class="text-[11px] text-destructive">{stopError || actionError}</p>
    </div>
  {/if}

  <!-- Actions bar -->
  <div class="border-t border-border/40 px-2 py-1.5 flex items-center gap-0.5" onclick={(e) => e.stopPropagation()}>
    {#if confirmVisible}
      <span class="text-[11px] text-muted-foreground pl-1">{$t("agents.stop_confirm")}</span>
      <Button size="sm" variant="destructive" class="h-6 px-2 text-[11px] ml-auto" onclick={handleStop} disabled={stopping} data-testid="agent-stop-confirm-btn">
        {stopping ? $t("agents.stopping") : $t("common.confirm")}
      </Button>
      <Button size="sm" variant="ghost" class="h-6 px-2 text-[11px]" onclick={() => { confirmVisible = false; }}>
        {$t("common.cancel")}
      </Button>
    {:else if uninstallConfirmVisible}
      <div class="flex flex-col gap-1.5 w-full py-0.5">
        <span class="text-[11px] font-medium text-destructive pl-1">
          {$t("agents.uninstall_confirm_warning", { values: { name: agent.name } })}
        </span>
        <label class="flex items-center gap-1.5 pl-1 cursor-pointer select-none" onclick={(e) => e.stopPropagation()}>
          <input type="checkbox" bind:checked={deleteMemoryData} class="h-3 w-3 rounded accent-destructive" />
          <span class="text-[10px] text-muted-foreground">{$t("agents.uninstall_delete_memory")}</span>
        </label>
        <div class="flex items-center gap-1 pl-1">
          <Button size="sm" variant="destructive" class="h-6 px-2 text-[11px]" onclick={confirmUninstall} disabled={uninstallLoading} data-testid="agent-uninstall-confirm-btn">
            {uninstallLoading ? $t("common.loading") : $t("agents.uninstall_confirm_action")}
          </Button>
          <Button size="sm" variant="ghost" class="h-6 px-2 text-[11px]" onclick={() => { uninstallConfirmVisible = false; deleteMemoryData = false; }}>
            {$t("common.cancel")}
          </Button>
        </div>
      </div>
    {:else}
      <!-- Primary actions -->
      {#if isRunning && agent.id && onchat}
        <Button size="sm" variant="ghost" class="h-6 px-2 text-[11px] gap-1" onclick={() => onchat(agent.name)} data-testid="agent-chat-button-{agent.name}">
          <MessageSquare size={10} /> {$t("nav.chat")}
        </Button>
      {/if}
      {#if isRunning && agent.id}
        <Button size="sm" variant="ghost" class="h-6 px-2 text-[11px] gap-1" onclick={() => { confirmVisible = true; }} data-testid="agent-stop-btn">
          <Square size={10} /> {$t("agents.stop")}
        </Button>
      {/if}
      {#if !isRunning && isInstalled && agent.install_path}
        <Button size="sm" variant="ghost" class="h-6 px-2 text-[11px] gap-1" onclick={handleStart} disabled={startLoading} data-testid="agent-start-btn">
          <Play size={10} /> {startLoading ? $t("agents.starting_agent") : $t("agents.start")}
        </Button>
      {/if}
      {#if agent.id}
        <Button size="sm" variant="ghost" class="h-6 px-2 text-[11px]" onclick={() => { if (agent.id) onlogs(agent.id); }} data-testid="agent-logs-btn">
          {$t("agents.logs")}
        </Button>
      {/if}

      <!-- Secondary actions -->
      <div class="ml-auto flex items-center gap-0.5">
        {#if isInstalled}
          <Button size="sm" variant="ghost" class="h-6 w-6 p-0" onclick={handleUpdate} disabled={updateLoading} aria-label={$t("agents.update")} data-testid="agent-update-button">
            <RefreshCw size={11} class="text-muted-foreground" />
          </Button>
          <Button size="sm" variant="ghost" class="h-6 w-6 p-0" onclick={handleUninstall} disabled={uninstallLoading} aria-label={$t("agents.uninstall")} data-testid="agent-uninstall-button">
            <Trash2 size={11} class="text-destructive/60" />
          </Button>
        {:else}
          <Button size="sm" variant="ghost" class="h-6 px-2 text-[11px]" onclick={handleInstallPermanently} disabled={installLoading} data-testid="install-permanently-btn">
            {installLoading ? $t("common.loading") : $t("agents.install_permanently")}
          </Button>
        {/if}
      </div>
    {/if}
  </div>
</div>
