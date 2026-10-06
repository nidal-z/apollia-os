<script lang="ts">
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { developerMode } from "$lib/stores/developerMode";
  import { navigateTo } from "$lib/stores/navigation";
  import { requestAgentSheet } from "$lib/stores/agentFocus";
  import { Sheet, SheetContent } from "$lib/components/ui/sheet";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { ExternalLink, Tag, Cpu, MessageSquare } from "lucide-svelte";
  import AgentActivity from "./AgentActivity.svelte";
  import AgentTriggers from "./AgentTriggers.svelte";
  import AgentMessagesPanel from "./AgentMessagesPanel.svelte";
  import { Card } from "$lib/components/ui/card";
  import { availability, statusLabel, type Availability } from "./agentStatus";

  interface Props {
    agent: AgentListItem;
    open: boolean;
    onclose: () => void;
    onlogs: (agentId: string) => void;
  }

  let { agent, open, onclose, onlogs }: Props = $props();

  const STATUS_VARIANT: Record<Availability, "success" | "warning" | "secondary"> = {
    available: "success",
    degraded: "warning",
    unavailable: "secondary",
  };

  const status = $derived(availability(agent));
  const isAvailable = $derived(status !== "unavailable");
  const showMessagesSection = $derived(agent.supports_a2a === true && $developerMode);

  function executionModeLabel(mode: string | null): string {
    switch (mode) {
      case "direct": return $t("agent_detail.execution_mode_direct");
      case "orchestrated": return $t("agent_detail.execution_mode_orchestrated");
      default: return $t("agent_detail.execution_mode_auto");
    }
  }

  // A run is read on the sheet of its agent, on the Agents screen.
  function handleTaskClick(_taskId: string) {
    onclose();
    if (agent.id) requestAgentSheet(agent.id);
    navigateTo("agents");
  }
  function handleMemoryLink() { onclose(); navigateTo("memory"); }
  function handleLogsClick() { if (agent.id) { onlogs(agent.id); } }
</script>

<Sheet {open} {onclose} class="w-full sm:max-w-[560px]">
  <div class="flex h-full flex-col" data-testid="agent-detail-sheet" data-agent-name={agent.name}>

    <!-- ═══ HEADER - glass card with brand wash ═══ -->
    <Card class="mx-4 mt-6 overflow-hidden">
      <!-- Accent bar -->
      <div class="h-0.5 w-full {isAvailable ? 'bg-primary' : 'bg-muted'}"></div>

      <div class="px-4 py-4">
        <div class="flex items-center gap-3">
          <!-- Colored avatar matching dashboard -->
          <Avatar name={agent.name} size="lg" ring />
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <h2 class="truncate text-base font-medium" data-testid="agent-detail-name">{agent.name}</h2>
              <Badge variant={STATUS_VARIANT[status]} class="text-micro-xs px-1.5 py-0" data-testid="agent-detail-status">{statusLabel(agent, $t)}</Badge>
            </div>
            <p class="mt-0.5 text-xs text-muted-foreground">v{agent.version}</p>
          </div>
        </div>

        <!-- Actions row -->
        {#if agent.id}
          <div class="mt-3 flex items-center gap-2">
            <Button size="sm" variant="ghost" onclick={handleLogsClick} data-testid="agent-detail-logs-btn">{$t('agents.logs')}</Button>
          </div>
        {/if}
      </div>
    </Card>

    <!-- ═══ SCROLLABLE CONTENT ═══ -->
    <SheetContent padding="flush" class="px-4 pt-4 pb-6 space-y-3">

      <!-- Description card -->
      {#if agent.description}
        <Card class="rounded-lg px-4 py-3.5" data-testid="agent-detail-description">
          <p class="text-body-sm text-foreground/85 leading-relaxed">{agent.description}</p>
        </Card>
      {/if}

      <!-- Info grid: mode + version details -->
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
        {#if agent.execution_mode}
          <Card class="rounded-lg px-3.5 py-3" data-testid="agent-detail-execution-mode">
            <div class="flex items-center gap-2 mb-1.5">
              <Cpu size={12} class="text-muted-foreground/50" />
              <span class="text-micro font-medium uppercase tracking-wider text-muted-foreground/50">{$t('agent_detail.execution_mode_title')}</span>
            </div>
            <p class="text-sm text-foreground/80">{executionModeLabel(agent.execution_mode)}</p>
          </Card>
        {/if}
      </div>

      <!-- Tags -->
      {#if agent.tags.length > 0}
        <Card class="rounded-lg px-4 py-3.5" data-testid="agent-detail-tags">
          <div class="flex items-center gap-2 mb-2.5">
            <Tag size={12} class="text-muted-foreground/50" />
            <span class="text-micro font-medium uppercase tracking-wider text-muted-foreground/50">{$t('agent_detail.tags_title')}</span>
          </div>
          <div class="flex flex-wrap gap-1.5">
            {#each agent.tags as tag}
              <span class="rounded-md bg-muted/50 px-2 py-0.5 text-caption text-foreground/65">{tag}</span>
            {/each}
          </div>
        </Card>
      {/if}

      <!-- Activity -->
      {#if agent.id}
        <Card class="rounded-lg px-4 py-3.5">
          <AgentActivity agentId={agent.id} onTaskClick={handleTaskClick} />
        </Card>
      {/if}

      <!-- Triggers -->
      <Card class="rounded-lg px-4 py-3.5">
        <AgentTriggers agentName={agent.name} />
      </Card>

      <!-- Agent messages (A2A + builder mode only) -->
      {#if showMessagesSection}
        <Card class="rounded-lg px-4 py-3.5" data-testid="agent-messages-tab">
          <div class="flex items-center gap-2 mb-3">
            <MessageSquare size={12} class="text-muted-foreground/50" />
            <span class="text-micro font-medium uppercase tracking-wider text-muted-foreground/50">{$t('agent_detail.messages_title')}</span>
          </div>
          <AgentMessagesPanel agentName={agent.name} />
        </Card>
      {/if}

      <!-- Memory link (builder only) -->
      {#if $developerMode}
        <button
          class="flex w-full items-center gap-2 rounded-lg glass-card glass-border px-4 py-3 text-sm text-primary hover:bg-primary/5 transition-colors"
          onclick={handleMemoryLink}
          data-testid="agent-detail-memory-link"
        >
          <ExternalLink size={13} />
          {$t('agent_detail.memory_link', { values: { namespace: agent.name } })}
        </button>
      {/if}
    </SheetContent>
  </div>
</Sheet>
