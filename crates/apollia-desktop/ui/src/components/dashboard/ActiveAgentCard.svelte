<script lang="ts">
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Bot } from "lucide-svelte";

  interface Props {
    agent: AgentListItem;
    ondetail: (agent: AgentListItem) => void;
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
    STATUS_BADGE[agent.runtime_status as "active" | "degraded"] ?? STATUS_BADGE.active,
  );

  function handleClick() {
    ondetail(agent);
  }
</script>

<button class="w-full text-left" onclick={handleClick} data-testid="active-agent-card" data-agent-name={agent.name}>
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
      <p class="text-xs text-muted-foreground">v{agent.version}</p>
    </CardContent>
  </Card>
</button>
