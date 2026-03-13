<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { AgentStatus } from "$lib/types";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    agent: AgentStatus;
    onlogs: (agentId: string) => void;
  }

  let { agent, onlogs }: Props = $props();

  const STATUS_CONFIG: Record<
    AgentStatus["state"],
    { label: string; variant: "default" | "secondary" | "destructive" | "outline"; extraClass: string }
  > = {
    active: { label: "ACTIVE", variant: "default", extraClass: "bg-[var(--apollia-success)] text-white" },
    degraded: { label: "DEGRADED", variant: "outline", extraClass: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]" },
    stopped: { label: "STOPPED", variant: "secondary", extraClass: "" },
    initializing: { label: "INITIALIZING", variant: "outline", extraClass: "animate-pulse border-blue-500 text-blue-500" },
    stopping: { label: "STOPPING", variant: "outline", extraClass: "border-orange-300 text-orange-300" },
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

<Card class="relative overflow-hidden">
  {#if agent.state === "degraded" && agent.degraded_reason}
    <div class="flex items-center gap-2 bg-[var(--apollia-warning)]/10 px-4 py-2 text-xs text-[var(--apollia-warning)]">
      <span>Warning: {agent.degraded_reason}</span>
    </div>
  {/if}

  <CardHeader class="pb-2">
    <div class="flex items-center justify-between">
      <CardTitle class="text-base font-semibold">{agent.name}</CardTitle>
      <Badge variant={config.variant} class={config.extraClass}>
        {config.label}
      </Badge>
    </div>
  </CardHeader>

  <CardContent>
    <div class="space-y-3">
      <!-- Stats row -->
      <div class="flex items-center gap-4 text-xs text-muted-foreground">
        {#if !isStopped(agent.state)}
          <span>Uptime: {formatUptime(agent.uptime_secs)}</span>
        {/if}
        <span>Completed: {agent.tasks_completed}</span>
        {#if agent.tasks_failed > 0}
          <span class="text-[hsl(var(--destructive))]">Failed: {agent.tasks_failed}</span>
        {:else}
          <span>Failed: {agent.tasks_failed}</span>
        {/if}
      </div>

      {#if stopError}
        <p class="text-xs text-[hsl(var(--destructive))]">{stopError}</p>
      {/if}

      <!-- Actions row -->
      <div class="flex items-center gap-2">
        {#if confirmVisible}
          <span class="text-xs text-muted-foreground">Stop this agent?</span>
          <Button size="sm" variant="destructive" onclick={handleStop} disabled={stopping}>
            {stopping ? "Stopping..." : "Confirm"}
          </Button>
          <Button size="sm" variant="outline" onclick={handleCancelStop}>
            Cancel
          </Button>
        {:else}
          {#if isRunning(agent.state)}
            <Button size="sm" variant="outline" onclick={handleStopClick}>
              Stop
            </Button>
          {/if}
          {#if !isStopped(agent.state)}
            <Button size="sm" variant="ghost" onclick={handleLogsClick}>
              Logs
            </Button>
          {/if}
        {/if}
      </div>
    </div>
  </CardContent>
</Card>
