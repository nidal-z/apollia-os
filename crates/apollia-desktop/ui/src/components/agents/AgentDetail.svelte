<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import { currentRoute } from "$lib/stores/navigation";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Separator } from "$lib/components/ui/separator";
  import { Bot, ExternalLink, Play, Square, Wrench, Tag } from "lucide-svelte";
  import AgentActivity from "./AgentActivity.svelte";
  import AgentTriggers from "./AgentTriggers.svelte";
  import AgentLlmInfo from "./AgentLlmInfo.svelte";

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
    { labelKey: string; variant: "default" | "secondary" | "destructive" | "outline"; extraClass: string }
  > = {
    active: { labelKey: "common.status.active", variant: "default", extraClass: "bg-[var(--apollia-success)] text-white" },
    degraded: { labelKey: "common.status.degraded", variant: "outline", extraClass: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]" },
    stopped: { labelKey: "common.status.stopped", variant: "secondary", extraClass: "" },
    initializing: { labelKey: "common.status.initializing", variant: "outline", extraClass: "animate-pulse border-info text-info" },
    stopping: { labelKey: "common.status.stopping", variant: "outline", extraClass: "border-warning text-warning" },
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
  const allTools = $derived([
    ...agent.tools_required.map((t) => ({ name: t, required: true })),
    ...agent.tools_optional.map((t) => ({ name: t, required: false })),
  ]);

  function executionModeLabel(mode: string | null): string {
    switch (mode) {
      case "direct":
        return $t("agent_detail.execution_mode_direct");
      case "orchestrated":
        return $t("agent_detail.execution_mode_orchestrated");
      default:
        return $t("agent_detail.execution_mode_auto");
    }
  }

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

  function handleTaskClick(_taskId: string) {
    onclose();
    currentRoute.set("tasks");
  }

  function handleMemoryLink() {
    onclose();
    currentRoute.set("memory");
  }

  function handleLogsClick() {
    if (agent.id) {
      onlogs(agent.id);
    }
  }
</script>

<Sheet {open} {onclose} class="w-[600px]">
  <div class="flex h-full flex-col" data-testid="agent-detail-sheet" data-agent-name={agent.name}>
    <!-- Header with agent identity -->
    <div class="px-5 py-4">
      <div class="flex items-center gap-3">
        <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
          <Bot size={20} class="text-primary" />
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <h2 class="truncate text-lg font-semibold" data-testid="agent-detail-name">{agent.name}</h2>
            {#if !isInstalled}
              <Badge variant="outline" class="whitespace-nowrap text-[10px] px-1.5 py-0">
                {$t("agents.session_only")}
              </Badge>
            {/if}
            {#if config}
              <Badge variant={config.variant} class="whitespace-nowrap text-[10px] px-1.5 py-0 {config.extraClass}" data-testid="agent-detail-status">
                {$t(config.labelKey)}
              </Badge>
            {:else}
              <Badge variant="secondary" class="whitespace-nowrap text-[10px] px-1.5 py-0" data-testid="agent-detail-status">
                {$t("agents.not_loaded")}
              </Badge>
            {/if}
          </div>
          <p class="text-xs text-muted-foreground">
            v{agent.version}
            {#if isInstalled}
              · {agent.enabled ? $t("agents.auto_start_enabled") : $t("agents.auto_start_disabled")}
            {/if}
          </p>
        </div>
      </div>

      {#if stopError || actionError}
        <div class="mt-2 rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-3 py-1.5 text-xs text-[hsl(var(--destructive))]">
          {stopError || actionError}
        </div>
      {/if}

      <!-- Action buttons -->
      <div class="mt-3 flex items-center gap-2">
        {#if confirmVisible}
          <span class="text-xs text-muted-foreground">{$t('agents.stop_confirm')}</span>
          <Button size="sm" variant="destructive" onclick={handleStop} disabled={stopping} data-testid="agent-detail-stop-confirm">
            {stopping ? $t('agents.stopping') : $t('common.confirm')}
          </Button>
          <Button size="sm" variant="outline" onclick={() => { confirmVisible = false; }}>
            {$t('common.cancel')}
          </Button>
        {:else}
          <!-- Start button for installed but not loaded agents -->
          {#if !isLoaded && isInstalled && agent.install_path}
            <Button size="sm" variant="default" onclick={handleStart} disabled={startLoading} data-testid="agent-detail-start-btn">
              <Play size={14} class="mr-1" />
              {startLoading ? $t('agents.starting_agent') : $t('agents.start')}
            </Button>
          {/if}
          {#if isRunning && agent.id}
            <Button size="sm" variant="outline" onclick={() => { confirmVisible = true; }} data-testid="agent-detail-stop-btn">
              <Square size={14} class="mr-1" />
              {$t('agents.stop')}
            </Button>
          {/if}
          {#if agent.id}
            <Button size="sm" variant="ghost" onclick={handleLogsClick} data-testid="agent-detail-logs-btn">
              {$t('agents.logs')}
            </Button>
          {/if}
          <!-- Enabled toggle for installed agents -->
          {#if isInstalled}
            <div class="ml-auto flex items-center gap-2">
              <span class="text-xs text-muted-foreground">
                {agent.enabled ? $t("agents.auto_start_enabled") : $t("agents.auto_start_disabled")}
              </span>
              <button
                class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {agent.enabled ? 'bg-primary' : 'bg-muted-foreground/30'}"
                onclick={handleToggleEnabled}
                disabled={toggleLoading}
                title={agent.enabled ? $t("agents.disable_tooltip") : $t("agents.enable_tooltip")}
                data-testid="agent-detail-enabled-toggle"
              >
                <span
                  class="inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform {agent.enabled ? 'translate-x-[18px]' : 'translate-x-[3px]'}"
                ></span>
              </button>
            </div>
          {/if}
          <Button size="sm" variant="ghost" onclick={onclose}>
            {$t('common.close')}
          </Button>
        {/if}
      </div>
    </div>

    <Separator />

    <!-- Scrollable content with all sections -->
    <div class="flex-1 space-y-6 overflow-auto px-5 py-4">
      <!-- Description -->
      {#if agent.description}
        <section data-testid="agent-detail-description">
          <h3 class="mb-2 text-sm font-semibold">{$t('agent_detail.description_title')}</h3>
          <p class="text-sm text-muted-foreground leading-relaxed">{agent.description}</p>
        </section>
        <Separator />
      {/if}

      <!-- Tags -->
      {#if agent.tags.length > 0}
        <section data-testid="agent-detail-tags">
          <h3 class="mb-2 text-sm font-semibold">{$t('agent_detail.tags_title')}</h3>
          <div class="flex flex-wrap gap-1.5">
            {#each agent.tags as tag}
              <Badge variant="outline" class="gap-1 text-xs">
                <Tag size={10} />
                {tag}
              </Badge>
            {/each}
          </div>
        </section>
        <Separator />
      {/if}

      <!-- Execution mode -->
      {#if agent.execution_mode}
        <section data-testid="agent-detail-execution-mode">
          <h3 class="mb-2 text-sm font-semibold">{$t('agent_detail.execution_mode_title')}</h3>
          <p class="text-sm text-muted-foreground">{executionModeLabel(agent.execution_mode)}</p>
        </section>
        <Separator />
      {/if}

      <!-- Tools -->
      {#if allTools.length > 0}
        <section data-testid="agent-detail-tools">
          <h3 class="mb-2 text-sm font-semibold">{$t('agent_detail.tools_title')}</h3>
          <div class="space-y-1.5">
            {#each allTools as tool}
              <div class="flex items-center gap-2 rounded-md border px-3 py-1.5">
                <Wrench size={12} class="shrink-0 text-muted-foreground" />
                <span class="text-sm">{tool.name}</span>
                <Badge variant={tool.required ? "default" : "outline"} class="ml-auto text-[10px] px-1.5 py-0">
                  {tool.required ? $t('agent_detail.tools_required_label') : $t('agent_detail.tools_optional_label')}
                </Badge>
              </div>
            {/each}
          </div>
        </section>
        <Separator />
      {/if}

      <!-- Recent activity (only if agent is loaded in runtime) -->
      {#if agent.id}
        <AgentActivity agentId={agent.id} onTaskClick={handleTaskClick} />
        <Separator />
      {/if}

      <!-- Triggers in natural language -->
      <AgentTriggers agentName={agent.name} />

      <Separator />

      <!-- AI Model -->
      <AgentLlmInfo />

      <!-- Install path (operator mode — useful context) -->
      {#if agent.install_path}
        <Separator />
        <section data-testid="agent-detail-install-path">
          <h3 class="mb-2 text-sm font-semibold">{$t('agent_detail.install_path_title')}</h3>
          <code class="block rounded-md bg-muted px-3 py-2 text-xs text-muted-foreground break-all">{agent.install_path}</code>
        </section>
      {/if}

      <!-- Memory (builder only) -->
      {#if $uiMode === "builder"}
        <Separator />
        <section data-testid="agent-detail-memory">
          <h3 class="mb-3 text-sm font-semibold">{$t('agent_detail.memory_title')}</h3>
          <button
            class="flex items-center gap-2 text-sm text-primary hover:underline"
            onclick={handleMemoryLink}
            data-testid="agent-detail-memory-link"
          >
            <ExternalLink size={14} />
            {$t('agent_detail.memory_link', { values: { namespace: agent.name } })}
          </button>
        </section>
      {/if}
    </div>
  </div>
</Sheet>
