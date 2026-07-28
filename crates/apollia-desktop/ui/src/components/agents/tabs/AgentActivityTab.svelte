<script lang="ts">
  /**
   * AgentActivityTab - recent tasks (via AgentActivity) plus the agent's
   * triggers (via AgentTriggers). Both are existing components reused as-is.
   */
  import { t } from "svelte-i18n";
  import { Card } from "$lib/components/operator";
  import AgentActivity from "../AgentActivity.svelte";
  import AgentTriggers from "../AgentTriggers.svelte";
  import type { AgentListItem } from "$lib/types";

  interface Props {
    agent: AgentListItem;
    onTaskClick: (taskId: string) => void;
  }

  let { agent, onTaskClick }: Props = $props();
</script>

<div class="max-w-3xl space-y-4" data-testid="agent-detail-activity">
  <Card class="p-3.5">
    {#if agent.id}
      <AgentActivity agentId={agent.id} {onTaskClick} />
    {:else}
      <p class="text-body-xs italic text-muted-foreground">
        {$t("agents.activity_not_loaded")}
      </p>
    {/if}
  </Card>

  <Card class="p-3.5">
    <AgentTriggers agentName={agent.name} />
  </Card>
</div>
