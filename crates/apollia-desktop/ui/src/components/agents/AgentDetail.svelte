<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AgentStatus } from "$lib/types";
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
    agent: AgentStatus;
    open: boolean;
    onclose: () => void;
    onlogs: (agentId: string) => void;
  }

  let { agent, open, onclose, onlogs }: Props = $props();

  const STATUS_CONFIG: Record<
    AgentStatus["state"],
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

  const config = $derived(STATUS_CONFIG[agent.state]);

  function isRunning(state: AgentStatus["state"]): boolean {
    return state === "active" || state === "degraded";
  }

  function formatUptime(totalSeconds: number): string {
    if (totalSeconds < 60) return `${totalSeconds}s`;
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
  }

  async function handleStop() {
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
    onlogs(agent.id);
  }
</script>

<Sheet {open} {onclose} class="w-[600px]">
  <div class="flex h-full flex-col" data-testid="agent-detail-sheet" data-agent-id={agent.id}>
    <!-- AC-1: Header with agent identity -->
    <div class="px-5 py-4">
      <div class="flex items-center gap-3">
        <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
          <Bot size={20} class="text-primary" />
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <h2 class="truncate text-lg font-semibold" data-testid="agent-detail-name">{agent.name}</h2>
            <Badge variant={config.variant} class={config.extraClass} data-testid="agent-detail-status">
              {$t(config.labelKey)}
            </Badge>
          </div>
          {#if agent.state !== "stopped"}
            <p class="text-xs text-muted-foreground">
              {$t('agents.uptime')}: {formatUptime(agent.uptime_secs)}
              · {$t('agents.completed')}: {agent.tasks_completed}
              {#if agent.tasks_failed > 0}
                · <span class="text-[hsl(var(--destructive))]">{$t('agents.failed')}: {agent.tasks_failed}</span>
              {/if}
            </p>
          {/if}
        </div>
      </div>

      {#if agent.state === "degraded" && agent.degraded_reason}
        <div class="mt-2 rounded-md bg-[var(--apollia-warning)]/10 px-3 py-1.5 text-xs text-[var(--apollia-warning)]">
          {$t('common.warning')}: {agent.degraded_reason}
        </div>
      {/if}

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
          {#if isRunning(agent.state)}
            <Button size="sm" variant="outline" onclick={() => { confirmVisible = true; }} data-testid="agent-detail-stop-btn">
              {$t('agents.stop')}
            </Button>
          {/if}
          <Button size="sm" variant="ghost" onclick={handleLogsClick} data-testid="agent-detail-logs-btn">
            {$t('agents.logs')}
          </Button>
          <Button size="sm" variant="ghost" onclick={onclose}>
            {$t('common.close')}
          </Button>
        {/if}
      </div>
    </div>

    <Separator />

    <!-- Scrollable content with all sections -->
    <div class="flex-1 space-y-6 overflow-auto px-5 py-4">
      <!-- AC-2: Recent activity -->
      <AgentActivity agentId={agent.id} onTaskClick={handleTaskClick} />

      <Separator />

      <!-- AC-3: Triggers in natural language -->
      <AgentTriggers agentName={agent.name} />

      <Separator />

      <!-- AC-4: AI Model -->
      <AgentLlmInfo />

      <!-- AC-5: Memory (builder only) -->
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
