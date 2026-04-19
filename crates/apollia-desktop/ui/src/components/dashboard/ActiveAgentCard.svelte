<script lang="ts">
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { tasks } from "$lib/stores/tasks";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Badge } from "$lib/components/ui/badge";
  import { CheckCircle, AlertTriangle, Zap } from "lucide-svelte";

  interface Props {
    agent: AgentListItem;
    ondetail: (agent: AgentListItem) => void;
  }

  let { agent, ondetail }: Props = $props();

  const isActive = $derived(agent.runtime_status === "active");
  const isDegraded = $derived(agent.runtime_status === "degraded");

  let agentTasks = $derived(
    agent.id ? $tasks.filter((t) => t.agent_id === agent.id) : [],
  );
  let completedCount = $derived(agentTasks.filter((t) => t.status === "completed").length);
  let failedCount = $derived(agentTasks.filter((t) => t.status === "failed").length);

</script>

<button
  class="group w-full h-full text-left"
  onclick={() => ondetail(agent)}
  data-testid="active-agent-card"
  data-agent-name={agent.name}
>
  <div class="glass-card-hover glass-border rounded-lg overflow-hidden h-full flex flex-col">
    <!-- Flat status accent -->
    <div
      class="h-0.5 w-full {isActive ? 'bg-primary' : isDegraded ? 'bg-warning' : 'bg-muted'}"
    ></div>

    <!-- Content — grows to fill -->
    <div class="px-3.5 pt-3 pb-2.5 flex-1 flex flex-col">
      <!-- Row 1: avatar + name + status -->
      <div class="flex items-center gap-2.5">
        <Avatar name={agent.name} size="md" ring />
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <span class="truncate text-[13px] font-medium group-hover:text-primary transition-colors" data-testid="active-agent-name">
              {agent.name}
            </span>
            <span class="shrink-0 text-[10px] text-muted-foreground/50">v{agent.version}</span>
          </div>
        </div>
        {#if isActive}
          <Badge variant="success" class="shrink-0 text-[9px] px-1.5 py-0">{$t("common.status.active")}</Badge>
        {:else if isDegraded}
          <Badge variant="warning" class="shrink-0 text-[9px] px-1.5 py-0">{$t("common.status.degraded")}</Badge>
        {/if}
      </div>

      <!-- Description — fixed height zone -->
      <p class="mt-2 line-clamp-2 text-xs text-muted-foreground leading-relaxed flex-1">
        {agent.description ?? ""}
      </p>

      <!-- Tags + stats — always at bottom -->
      <div class="mt-2 flex items-end justify-between">
        <div class="flex flex-wrap gap-1">
          {#each agent.tags.slice(0, 3) as tag}
            <span class="rounded bg-muted/60 px-1.5 py-px text-[10px] text-muted-foreground">{tag}</span>
          {/each}
          {#if agent.tags.length > 3}
            <span class="text-[10px] text-muted-foreground/40">+{agent.tags.length - 3}</span>
          {/if}
        </div>
        <div class="flex items-center gap-2.5 text-[10px] text-muted-foreground/60">
          {#if completedCount > 0}
            <span class="flex items-center gap-0.5">
              <CheckCircle size={10} class="text-success" />
              {completedCount}
            </span>
          {/if}
          {#if failedCount > 0}
            <span class="flex items-center gap-0.5">
              <AlertTriangle size={10} class="text-destructive" />
              {failedCount}
            </span>
          {/if}
          {#if completedCount === 0 && failedCount === 0}
            <span class="flex items-center gap-0.5">
              <Zap size={10} />
              {$t('dashboard.ready')}
            </span>
          {/if}
        </div>
      </div>
    </div>
  </div>
</button>
