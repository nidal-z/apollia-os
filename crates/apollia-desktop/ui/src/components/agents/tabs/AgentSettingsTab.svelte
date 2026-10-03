<script lang="ts">
  /**
   * AgentSettingsTab - the logs entry, the LLM info panel, and the A2A message
   * log (builder-only). There is no start, stop nor auto-start here: an agent
   * is always available, loaded by its first task and released once idle.
   */
  import { t } from "svelte-i18n";
  import { MessageSquare } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { DeveloperOnly } from "$lib/components/shared";
  import AgentLlmInfo from "../AgentLlmInfo.svelte";
  import AgentMessagesPanel from "../AgentMessagesPanel.svelte";
  import type { AgentListItem } from "$lib/types";

  interface Props {
    agent: AgentListItem;
    onOpenLogs: (agentId: string) => void;
  }

  let { agent, onOpenLogs }: Props = $props();
</script>

<div class="max-w-3xl space-y-4" data-testid="agent-detail-settings">
  {#if agent.id}
    {@const agentId = agent.id}
    <Card class="p-3.5">
      <div class="mb-3 font-mono text-overline uppercase text-muted-foreground">
        {$t("agents.execution_section")}
      </div>
      <Button
        size="sm"
        variant="ghost"
        onclick={() => onOpenLogs(agentId)}
        data-testid="agent-detail-logs-btn"
      >
        {$t("agents.logs")}
      </Button>
    </Card>
  {/if}

  <Card class="p-3.5">
    <AgentLlmInfo />
  </Card>

  {#if agent.supports_a2a}
    <DeveloperOnly>
      <Card class="p-3.5">
        <div class="mb-3 flex items-center gap-2">
          <MessageSquare size={12} class="text-muted-foreground/50" />
          <span class="text-overline uppercase text-muted-foreground/50">
            {$t("agent_detail.messages_title")}
          </span>
        </div>
        <AgentMessagesPanel agentName={agent.name} />
      </Card>
    </DeveloperOnly>
  {/if}
</div>
