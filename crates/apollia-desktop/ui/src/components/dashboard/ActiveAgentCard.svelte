<script lang="ts">
  import { t } from "svelte-i18n";
  import type { AgentStatus } from "$lib/types";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Bot } from "lucide-svelte";

  interface Props {
    agent: AgentStatus;
    ondetail: (agent: AgentStatus) => void;
  }

  let { agent, ondetail }: Props = $props();

  const STATUS_BADGE: Record<
    "active" | "degraded",
    { labelKey: string; variant: "default" | "outline"; extraClass: string }
  > = {
    active: {
      labelKey: "common.status.active",
      variant: "default",
      extraClass: "bg-[var(--apollia-success)] text-white",
    },
    degraded: {
      labelKey: "common.status.degraded",
      variant: "outline",
      extraClass: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]",
    },
  };

  let badgeConfig = $derived(
    STATUS_BADGE[agent.state as "active" | "degraded"] ?? STATUS_BADGE.active,
  );

  function handleClick() {
    ondetail(agent);
  }
</script>

<button class="w-full text-left" onclick={handleClick} data-testid="active-agent-card" data-agent-id={agent.id}>
  <Card class="transition-colors hover:bg-[rgba(52,53,245,0.04)] dark:hover:bg-[rgba(124,95,214,0.06)]">
    <CardHeader class="pb-2">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <Bot size={16} class="text-muted-foreground" />
          <CardTitle class="text-sm font-semibold" data-testid="active-agent-name">{agent.name}</CardTitle>
        </div>
        <Badge variant={badgeConfig.variant} class={badgeConfig.extraClass}>
          {$t(badgeConfig.labelKey)}
        </Badge>
      </div>
    </CardHeader>
    <CardContent>
      <div class="flex items-center gap-4 text-xs text-muted-foreground">
        <span>{$t('agents.completed')}: {agent.tasks_completed}</span>
        {#if agent.tasks_failed > 0}
          <span class="text-[hsl(var(--destructive))]">{$t('agents.failed')}: {agent.tasks_failed}</span>
        {/if}
      </div>
    </CardContent>
  </Card>
</button>
