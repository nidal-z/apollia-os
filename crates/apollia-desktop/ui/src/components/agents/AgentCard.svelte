<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AgentStatus } from "$lib/types";
  import { Card } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { uiMode } from "$lib/stores/mode";

  interface Props {
    agent: AgentStatus;
    onlogs: (agentId: string) => void;
    ondetail: (agent: AgentStatus) => void;
  }

  let { agent, onlogs, ondetail }: Props = $props();

  const STATUS_CONFIG: Record<
    AgentStatus["state"],
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

  function agentColor(name: string): string {
    const hash = name.split("").reduce((acc, char) => acc + char.charCodeAt(0), 0);
    const hue = hash % 360;
    return `hsl(${hue}, 60%, 60%)`;
  }

  function agentInitial(name: string): string {
    return name.charAt(0).toUpperCase();
  }

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
  const displayDescription = $derived(agent.description || $t("agents.no_description"));
</script>

<Card
  class="relative cursor-pointer overflow-hidden transition-all duration-200 hover:bg-[rgba(52,53,245,0.04)] dark:hover:bg-[rgba(124,95,214,0.06)] hover:shadow-md"
  data-testid="agent-card"
  data-agent-id={agent.id}
  data-agent-state={agent.state}
  onclick={() => ondetail(agent)}
>
  <!-- AC-1: Avatar + Name + Description + Badge -->
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
          <Badge variant={config.variant} class={config.extraClass} data-testid="agent-status">
            {$t(config.labelKey)}
          </Badge>
        </div>

        <!-- Description (1-2 lines) -->
        <p class="mt-1 line-clamp-2 text-sm text-muted-foreground" data-testid="agent-description">
          {displayDescription}
        </p>
      </div>
    </div>
  </div>

  <!-- AC-2: Metrics adapted to mode -->
  <div class="border-t px-4 py-2.5">
    {#if $uiMode === "operator"}
      <span class="text-xs text-muted-foreground">
        {$t("agents.tasks_completed_count", { values: { count: agent.tasks_completed } })}
      </span>
    {:else}
      <div class="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
        <span>{$t("agents.completed")}: {agent.tasks_completed}</span>
        {#if agent.tasks_failed > 0}
          <span class="text-[hsl(var(--destructive))]">{$t("agents.failed")}: {agent.tasks_failed}</span>
        {:else}
          <span>{$t("agents.failed")}: {agent.tasks_failed}</span>
        {/if}
        {#if !isStopped(agent.state)}
          <span>{$t("agents.uptime")}: {formatUptime(agent.uptime_secs)}</span>
        {/if}
        {#if agent.degraded_reason}
          <span class="text-[var(--apollia-warning)]">{agent.degraded_reason}</span>
        {/if}
      </div>
    {/if}
  </div>

  <!-- AC-4: Degraded warning bar -->
  {#if agent.state === "degraded" && agent.degraded_reason}
    <div
      class="flex items-center gap-2 border-t border-[var(--apollia-warning)]/30 bg-[var(--apollia-warning)]/10 px-4 py-2 text-xs text-[var(--apollia-warning)]"
      data-testid="agent-degraded-warning"
    >
      <span>{$t("common.warning")}: {agent.degraded_reason}</span>
    </div>
  {/if}

  <!-- Actions (stop on click to prevent card navigation) -->
  {#if stopError}
    <div class="border-t px-4 py-2">
      <p class="text-xs text-[hsl(var(--destructive))]">{stopError}</p>
    </div>
  {/if}

  <div class="border-t px-4 py-2" onclick={(e) => e.stopPropagation()}>
    {#if confirmVisible}
      <div class="flex items-center gap-2">
        <span class="text-xs text-muted-foreground">{$t("agents.stop_confirm")}</span>
        <Button size="sm" variant="destructive" onclick={handleStop} disabled={stopping} data-testid="agent-stop-confirm-btn">
          {stopping ? $t("agents.stopping") : $t("common.confirm")}
        </Button>
        <Button size="sm" variant="outline" onclick={handleCancelStop}>
          {$t("common.cancel")}
        </Button>
      </div>
    {:else}
      <div class="flex items-center gap-2">
        {#if isRunning(agent.state)}
          <Button size="sm" variant="outline" onclick={handleStopClick} data-testid="agent-stop-btn">
            {$t("agents.stop")}
          </Button>
        {/if}
        {#if !isStopped(agent.state)}
          <Button size="sm" variant="ghost" onclick={handleLogsClick} data-testid="agent-logs-btn">
            {$t("agents.logs")}
          </Button>
        {/if}
      </div>
    {/if}
  </div>
</Card>
