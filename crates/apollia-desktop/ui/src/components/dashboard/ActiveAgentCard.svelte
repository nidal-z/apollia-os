<script lang="ts">
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { tasks } from "$lib/stores/tasks";
  import { Card } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Activity, CheckCircle, AlertTriangle } from "lucide-svelte";

  interface Props {
    agent: AgentListItem;
    ondetail: (agent: AgentListItem) => void;
  }

  let { agent, ondetail }: Props = $props();

  const STATUS_BADGE: Record<
    "active" | "degraded",
    { labelKey: string; variant: "default" | "outline"; extraClass: string; dotClass: string }
  > = {
    active: {
      labelKey: "common.status.active",
      variant: "default",
      extraClass: "bg-[var(--apollia-success)] text-white",
      dotClass: "bg-green-500",
    },
    degraded: {
      labelKey: "common.status.degraded",
      variant: "outline",
      extraClass: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]",
      dotClass: "bg-yellow-500",
    },
  };

  let badgeConfig = $derived(
    STATUS_BADGE[agent.runtime_status as "active" | "degraded"] ?? STATUS_BADGE.active,
  );

  let agentTasks = $derived(
    agent.id ? $tasks.filter((t) => t.agent_id === agent.id) : [],
  );
  let workingCount = $derived(agentTasks.filter((t) => t.status === "working").length);
  let completedCount = $derived(agentTasks.filter((t) => t.status === "completed").length);
  let failedCount = $derived(agentTasks.filter((t) => t.status === "failed").length);

  function agentColor(name: string): string {
    const hash = name.split("").reduce((acc, char) => acc + char.charCodeAt(0), 0);
    const hue = hash % 360;
    return `hsl(${hue}, 55%, 55%)`;
  }

  function agentInitial(name: string): string {
    return name.charAt(0).toUpperCase();
  }

  function handleClick() {
    ondetail(agent);
  }
</script>

<button class="w-full text-left" onclick={handleClick} data-testid="active-agent-card" data-agent-name={agent.name}>
  <Card class="relative overflow-hidden transition-all duration-200 hover:shadow-md hover:bg-[rgba(52,53,245,0.04)] dark:hover:bg-[rgba(124,95,214,0.06)]">
    <!-- Status indicator bar at top -->
    <div class="h-1 w-full {badgeConfig.dotClass}"></div>

    <div class="px-4 pt-3 pb-3">
      <!-- Header: avatar + name + badge -->
      <div class="flex items-start gap-3">
        <div
          class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-xs font-semibold text-white"
          style="background-color: {agentColor(agent.name)}"
        >
          {agentInitial(agent.name)}
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex items-center justify-between gap-2">
            <h3 class="truncate text-sm font-semibold" data-testid="active-agent-name">{agent.name}</h3>
            <Badge variant={badgeConfig.variant} class="shrink-0 whitespace-nowrap text-[10px] px-1.5 py-0 {badgeConfig.extraClass}">
              {$t(badgeConfig.labelKey)}
            </Badge>
          </div>
          <p class="mt-0.5 text-[11px] text-muted-foreground">v{agent.version}</p>
        </div>
      </div>

      <!-- Description -->
      {#if agent.description}
        <p class="mt-2 line-clamp-2 text-xs text-muted-foreground/80 leading-relaxed">
          {agent.description}
        </p>
      {/if}

      <!-- Tags (max 3) -->
      {#if agent.tags.length > 0}
        <div class="mt-2 flex flex-wrap gap-1">
          {#each agent.tags.slice(0, 3) as tag}
            <span class="rounded-full bg-muted px-1.5 py-0 text-[10px] text-muted-foreground">{tag}</span>
          {/each}
          {#if agent.tags.length > 3}
            <span class="text-[10px] text-muted-foreground">+{agent.tags.length - 3}</span>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Stats footer -->
    <div class="border-t bg-muted/30 px-4 py-2">
      <div class="flex items-center gap-4 text-[11px] text-muted-foreground">
        {#if workingCount > 0}
          <span class="flex items-center gap-1">
            <Activity size={11} class="animate-pulse text-blue-500" />
            <span class="font-medium text-blue-600 dark:text-blue-400">{workingCount} {$t('dashboard.in_progress_short')}</span>
          </span>
        {/if}
        <span class="flex items-center gap-1">
          <CheckCircle size={11} class="text-green-500" />
          {completedCount}
        </span>
        {#if failedCount > 0}
          <span class="flex items-center gap-1">
            <AlertTriangle size={11} class="text-red-500" />
            {failedCount}
          </span>
        {/if}
      </div>
    </div>
  </Card>
</button>
