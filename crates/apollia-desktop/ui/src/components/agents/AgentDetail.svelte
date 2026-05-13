<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import { navigateTo } from "$lib/stores/navigation";
  import { Sheet, SheetContent } from "$lib/components/ui/sheet";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { ExternalLink, Play, Square, Wrench, Tag, Cpu, Terminal, MessageSquare } from "lucide-svelte";
  import AgentActivity from "./AgentActivity.svelte";
  import AgentTriggers from "./AgentTriggers.svelte";
  import AgentLlmInfo from "./AgentLlmInfo.svelte";
  import AgentMessagesPanel from "./AgentMessagesPanel.svelte";
  import { Card } from "$lib/components/ui/card";

  interface Props {
    agent: AgentListItem;
    open: boolean;
    onclose: () => void;
    onlogs: (agentId: string) => void;
  }

  let { agent, open, onclose, onlogs }: Props = $props();

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
  let stopError = $state<string | null>(null);
  let startLoading = $state(false);
  let toggleLoading = $state(false);
  let actionError = $state<string | null>(null);

  const runtimeStatus = $derived(agent.runtime_status as RuntimeState | null);
  const config = $derived(runtimeStatus ? STATUS_CONFIG[runtimeStatus] : null);
  const isInstalled = $derived(agent.installed_at !== null);
  const isRunning = $derived(runtimeStatus === "active" || runtimeStatus === "degraded");
  const isLoaded = $derived(runtimeStatus !== null);
  const showMessagesSection = $derived(agent.supports_a2a === true && $uiMode === "builder");
  const allTools = $derived([
    ...agent.tools_required.map((t) => ({ name: t, required: true })),
    ...agent.tools_optional.map((t) => ({ name: t, required: false })),
  ]);

  function executionModeLabel(mode: string | null): string {
    switch (mode) {
      case "direct": return $t("agent_detail.execution_mode_direct");
      case "orchestrated": return $t("agent_detail.execution_mode_orchestrated");
      default: return $t("agent_detail.execution_mode_auto");
    }
  }

  async function handleStop() {
    if (!agent.id) return;
    confirmVisible = false;
    stopping = true;
    stopError = null;
    try { await invoke("stop_agent", { agentId: agent.id }); }
    catch (err: unknown) { stopError = err instanceof Error ? err.message : String(err); }
    finally { stopping = false; }
  }

  async function handleStart() {
    if (!agent.install_path) return;
    startLoading = true;
    actionError = null;
    try { await invoke("start_agent", { path: agent.install_path }); }
    catch (err: unknown) { actionError = err instanceof Error ? err.message : String(err); }
    finally { startLoading = false; }
  }

  async function handleToggleEnabled() {
    toggleLoading = true;
    actionError = null;
    try {
      if (agent.enabled) { await invoke("disable_agent", { name: agent.name }); }
      else { await invoke("enable_agent", { name: agent.name }); }
    }
    catch (err: unknown) { actionError = err instanceof Error ? err.message : String(err); }
    finally { toggleLoading = false; }
  }

  function handleTaskClick(_taskId: string) { onclose(); navigateTo("tasks"); }
  function handleMemoryLink() { onclose(); navigateTo("memory"); }
  function handleLogsClick() { if (agent.id) { onlogs(agent.id); } }
</script>

<Sheet {open} {onclose} class="w-full sm:max-w-[560px]">
  <div class="flex h-full flex-col" data-testid="agent-detail-sheet" data-agent-name={agent.name}>

    <!-- ═══ HEADER — glass card with brand wash ═══ -->
    <Card class="mx-4 mt-6 overflow-hidden">
      <!-- Accent bar -->
      <div class="h-0.5 w-full {isRunning ? 'bg-primary' : 'bg-muted'}"></div>

      <div class="px-4 py-4">
        <div class="flex items-center gap-3">
          <!-- Colored avatar matching dashboard -->
          <Avatar name={agent.name} size="lg" ring />
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <h2 class="truncate text-base font-medium" data-testid="agent-detail-name">{agent.name}</h2>
              {#if config}
                <Badge variant={config.variant} class="text-[9px] px-1.5 py-0" data-testid="agent-detail-status">{$t(config.labelKey)}</Badge>
              {:else}
                <Badge variant="secondary" class="text-[9px] px-1.5 py-0" data-testid="agent-detail-status">{$t("agents.not_loaded")}</Badge>
              {/if}
            </div>
            <p class="mt-0.5 text-xs text-muted-foreground">
              v{agent.version}
              {#if isInstalled}
                · {agent.enabled ? $t("agents.auto_start_enabled") : $t("agents.auto_start_disabled")}
              {/if}
            </p>
          </div>
        </div>

        <!-- Actions row -->
        <div class="mt-3 flex items-center gap-2">
          {#if confirmVisible}
            <span class="text-xs text-muted-foreground">{$t('agents.stop_confirm')}</span>
            <Button size="sm" variant="destructive" onclick={handleStop} disabled={stopping} data-testid="agent-detail-stop-confirm">
              {stopping ? $t('agents.stopping') : $t('common.confirm')}
            </Button>
            <Button size="sm" variant="outline" onclick={() => { confirmVisible = false; }}>{$t('common.cancel')}</Button>
          {:else}
            {#if !isLoaded && isInstalled && agent.install_path}
              <Button size="sm" onclick={handleStart} disabled={startLoading} data-testid="agent-detail-start-btn" class="gap-1">
                <Play size={12} /> {startLoading ? $t('agents.starting_agent') : $t('agents.start')}
              </Button>
            {/if}
            {#if isRunning && agent.id}
              <Button size="sm" variant="outline" onclick={() => { confirmVisible = true; }} data-testid="agent-detail-stop-btn" class="gap-1">
                <Square size={12} /> {$t('agents.stop')}
              </Button>
            {/if}
            {#if agent.id}
              <Button size="sm" variant="ghost" onclick={handleLogsClick} data-testid="agent-detail-logs-btn">{$t('agents.logs')}</Button>
            {/if}
            {#if isInstalled}
              <div class="ml-auto flex items-center gap-2">
                <span class="text-[11px] text-muted-foreground">{$t("agents.auto_start_enabled")}</span>
                <Toggle checked={agent.enabled} onchange={handleToggleEnabled} disabled={toggleLoading} size="sm" aria-label={$t("agents.auto_start_enabled")} data-testid="agent-detail-enabled-toggle" />
              </div>
            {/if}
          {/if}
        </div>
      </div>
    </Card>

    {#if stopError || actionError}
      <div class="mx-4 mt-2 rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
        {stopError || actionError}
      </div>
    {/if}

    <!-- ═══ SCROLLABLE CONTENT ═══ -->
    <SheetContent padding="flush" class="px-4 pt-4 pb-6 space-y-3">

      <!-- Description card -->
      {#if agent.description}
        <Card class="rounded-lg px-4 py-3.5" data-testid="agent-detail-description">
          <p class="text-[13px] text-foreground/85 leading-relaxed">{agent.description}</p>
        </Card>
      {/if}

      <!-- Info grid: mode + version details -->
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
        {#if agent.execution_mode}
          <Card class="rounded-lg px-3.5 py-3" data-testid="agent-detail-execution-mode">
            <div class="flex items-center gap-2 mb-1.5">
              <Cpu size={12} class="text-muted-foreground/50" />
              <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('agent_detail.execution_mode_title')}</span>
            </div>
            <p class="text-sm text-foreground/80">{executionModeLabel(agent.execution_mode)}</p>
          </Card>
        {/if}
        {#if agent.install_path}
          <Card class="rounded-lg px-3.5 py-3" data-testid="agent-detail-install-path">
            <div class="flex items-center gap-2 mb-1.5">
              <Terminal size={12} class="text-muted-foreground/50" />
              <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('agent_detail.install_path_title')}</span>
            </div>
            <p class="text-xs text-muted-foreground font-mono truncate" title={agent.install_path}>{agent.install_path}</p>
          </Card>
        {/if}
      </div>

      <!-- Tags -->
      {#if agent.tags.length > 0}
        <Card class="rounded-lg px-4 py-3.5" data-testid="agent-detail-tags">
          <div class="flex items-center gap-2 mb-2.5">
            <Tag size={12} class="text-muted-foreground/50" />
            <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('agent_detail.tags_title')}</span>
          </div>
          <div class="flex flex-wrap gap-1.5">
            {#each agent.tags as tag}
              <span class="rounded-md bg-muted/50 px-2 py-0.5 text-[11px] text-foreground/65">{tag}</span>
            {/each}
          </div>
        </Card>
      {/if}

      <!-- Tools -->
      {#if allTools.length > 0}
        <Card class="rounded-lg overflow-hidden" data-testid="agent-detail-tools">
          <div class="flex items-center gap-2 px-4 pt-3.5 pb-2">
            <Wrench size={12} class="text-muted-foreground/50" />
            <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('agent_detail.tools_title')}</span>
            <span class="text-[10px] text-muted-foreground/30 ml-auto">{allTools.length}</span>
          </div>
          <div class="divide-y divide-border/40">
            {#each allTools as tool}
              <div class="flex items-center gap-3 px-4 py-2 transition-colors duration-150 hover:bg-primary/5">
                <span class="flex-1 text-[13px] text-foreground/80">{tool.name}</span>
                <Badge variant={tool.required ? "destructive" : "outline"} class="text-[9px] px-1.5 py-0">
                  {tool.required ? $t('agent_detail.tools_required_label') : $t('agent_detail.tools_optional_label')}
                </Badge>
              </div>
            {/each}
          </div>
        </Card>
      {/if}

      <!-- Activity -->
      {#if agent.id}
        <Card class="rounded-lg px-4 py-3.5">
          <AgentActivity agentId={agent.id} onTaskClick={handleTaskClick} />
        </Card>
      {/if}

      <!-- Triggers -->
      <Card class="rounded-lg px-4 py-3.5">
        <AgentTriggers agentName={agent.name} />
      </Card>

      <!-- LLM info -->
      <Card class="rounded-lg px-4 py-3.5">
        <AgentLlmInfo />
      </Card>

      <!-- Agent messages (A2A + builder mode only) -->
      {#if showMessagesSection}
        <Card class="rounded-lg px-4 py-3.5" data-testid="agent-messages-tab">
          <div class="flex items-center gap-2 mb-3">
            <MessageSquare size={12} class="text-muted-foreground/50" />
            <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('agent_detail.messages_title')}</span>
          </div>
          <AgentMessagesPanel agentName={agent.name} />
        </Card>
      {/if}

      <!-- Memory link (builder only) -->
      {#if $uiMode === "builder"}
        <button
          class="flex w-full items-center gap-2 rounded-lg glass-card glass-border px-4 py-3 text-sm text-primary hover:bg-primary/5 transition-colors"
          onclick={handleMemoryLink}
          data-testid="agent-detail-memory-link"
        >
          <ExternalLink size={13} />
          {$t('agent_detail.memory_link', { values: { namespace: agent.name } })}
        </button>
      {/if}
    </SheetContent>
  </div>
</Sheet>
