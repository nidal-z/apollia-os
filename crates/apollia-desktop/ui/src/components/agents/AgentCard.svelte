<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AgentStatus } from "$lib/types";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    agent: AgentStatus;
    onlogs: (agentId: string) => void;
    ondetail: (agent: AgentStatus) => void;
  }

  let { agent, onlogs, ondetail }: Props = $props();

  const STATUS_CONFIG: Record<
    AgentStatus["state"],
    { labelKey: string; variant: "default" | "secondary" | "destructive" | "outline"; extraClass: string }
  > = {
    active: { labelKey: "common.status.active", variant: "default", extraClass: "bg-[var(--apollia-success)] text-white" },
    degraded: { labelKey: "common.status.degraded", variant: "outline", extraClass: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]" },
    stopped: { labelKey: "common.status.stopped", variant: "secondary", extraClass: "" },
    initializing: { labelKey: "common.status.initializing", variant: "outline", extraClass: "animate-pulse border-blue-500 text-blue-500" },
    stopping: { labelKey: "common.status.stopping", variant: "outline", extraClass: "border-orange-300 text-orange-300" },
  };

  let stopping = $state(false);
  let confirmVisible = $state(false);
  let stopError = $state<string | null>(null);

  function formatUptime(totalSeconds: number): string {
    if (totalSeconds < 60) return `${totalSeconds}s`;
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
  }

  function isRunning(state: AgentStatus["state"]): boolean {
    return state === "active" || state === "degraded";
  }

  function isStopped(state: AgentStatus["state"]): boolean {
    return state === "stopped";
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

  function handleStopClick() {
    confirmVisible = true;
  }

  function handleCancelStop() {
    confirmVisible = false;
  }

  function handleLogsClick() {
    onlogs(agent.id);
  }

  const config = $derived(STATUS_CONFIG[agent.state]);
</script>

<Card class="relative overflow-hidden transition-colors hover:bg-accent/30" data-testid="agent-card" data-agent-id={agent.id} data-agent-state={agent.state}>
  {#if agent.state === "degraded" && agent.degraded_reason}
    <div class="flex items-center gap-2 bg-[var(--apollia-warning)]/10 px-4 py-2 text-xs text-[var(--apollia-warning)]">
      <span>{$t('common.warning')}: {agent.degraded_reason}</span>
    </div>
  {/if}

  <CardHeader class="cursor-pointer pb-2" onclick={() => ondetail(agent)}>
    <div class="flex items-center justify-between">
      <CardTitle class="text-base font-semibold" data-testid="agent-name">{agent.name}</CardTitle>
      <Badge variant={config.variant} class={config.extraClass} data-testid="agent-status">
        {$t(config.labelKey)}
      </Badge>
    </div>
  </CardHeader>

  <CardContent>
    <div class="space-y-3">
      <!-- Stats row -->
      <div class="flex items-center gap-4 text-xs text-muted-foreground">
        {#if !isStopped(agent.state)}
          <span>{$t('agents.uptime')}: {formatUptime(agent.uptime_secs)}</span>
        {/if}
        <span>{$t('agents.completed')}: {agent.tasks_completed}</span>
        {#if agent.tasks_failed > 0}
          <span class="text-[hsl(var(--destructive))]">{$t('agents.failed')}: {agent.tasks_failed}</span>
        {:else}
          <span>{$t('agents.failed')}: {agent.tasks_failed}</span>
        {/if}
      </div>

      {#if stopError}
        <p class="text-xs text-[hsl(var(--destructive))]">{stopError}</p>
      {/if}

      <!-- Actions row -->
      <div class="flex items-center gap-2">
        {#if confirmVisible}
          <span class="text-xs text-muted-foreground">{$t('agents.stop_confirm')}</span>
          <Button size="sm" variant="destructive" onclick={handleStop} disabled={stopping} data-testid="agent-stop-confirm-btn">
            {stopping ? $t('agents.stopping') : $t('common.confirm')}
          </Button>
          <Button size="sm" variant="outline" onclick={handleCancelStop}>
            {$t('common.cancel')}
          </Button>
        {:else}
          {#if isRunning(agent.state)}
            <Button size="sm" variant="outline" onclick={handleStopClick} data-testid="agent-stop-btn">
              {$t('agents.stop')}
            </Button>
          {/if}
          {#if !isStopped(agent.state)}
            <Button size="sm" variant="ghost" onclick={handleLogsClick} data-testid="agent-logs-btn">
              {$t('agents.logs')}
            </Button>
          {/if}
        {/if}
      </div>
    </div>
  </CardContent>
</Card>
