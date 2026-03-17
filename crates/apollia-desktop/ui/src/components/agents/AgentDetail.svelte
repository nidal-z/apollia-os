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
  import { Bot, ExternalLink } from "lucide-svelte";
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

  const runtimeStatus = $derived(agent.runtime_status as RuntimeState | null);
  const config = $derived(runtimeStatus ? STATUS_CONFIG[runtimeStatus] : null);
  const isInstalled = $derived(agent.installed_at !== null);
  const isRunning = $derived(runtimeStatus === "active" || runtimeStatus === "degraded");

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
              <Badge variant="outline" class="text-[10px]">
                {$t("agents.session_only")}
              </Badge>
            {/if}
            {#if config}
              <Badge variant={config.variant} class={config.extraClass} data-testid="agent-detail-status">
                {$t(config.labelKey)}
              </Badge>
            {:else}
              <Badge variant="secondary" data-testid="agent-detail-status">
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

      {#if stopError}
        <div class="mt-2 rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-3 py-1.5 text-xs text-[hsl(var(--destructive))]">
          {stopError}
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
          {#if isRunning && agent.id}
            <Button size="sm" variant="outline" onclick={() => { confirmVisible = true; }} data-testid="agent-detail-stop-btn">
              {$t('agents.stop')}
            </Button>
          {/if}
          {#if agent.id}
            <Button size="sm" variant="ghost" onclick={handleLogsClick} data-testid="agent-detail-logs-btn">
              {$t('agents.logs')}
            </Button>
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
