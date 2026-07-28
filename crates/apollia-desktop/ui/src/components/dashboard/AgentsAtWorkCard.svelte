<script lang="ts">
  /**
   * AgentsAtWorkCard - secondary bento card listing agents currently running.
   * Operator mode shows name + status; Builder mode adds the technical mono
   * line via BuilderOnly. Each row exposes detail / logs via right-click.
   */
  import { t } from "svelte-i18n";
  import { Sparkles, Eye, ScrollText } from "lucide-svelte";
  import type { AgentListItem } from "$lib/types";
  import { ListRow, StatusDot } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { BuilderOnly } from "$lib/components/shared";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import DashboardCardHeader from "./DashboardCardHeader.svelte";

  interface Props {
    agents: AgentListItem[];
    max?: number;
    onDetail: (agent: AgentListItem) => void;
    onLogs: (agentId: string) => void;
  }

  let { agents, max = 5, onDetail, onLogs }: Props = $props();

  const visible = $derived(agents.slice(0, max));
  const overflow = $derived(agents.length - max);

  function techLine(agent: AgentListItem): string {
    const parts = [`v${agent.version}`];
    if (agent.tags.length > 0) parts.push(...agent.tags.slice(0, 3));
    if (agent.agent_type === "worker") parts.push("WORKER");
    return parts.join(" · ");
  }

  function rowMenu(agent: AgentListItem): ActionMenuItem[] {
    return [
      {
        id: "detail",
        label: $t("dashboard.ctx_agent_detail"),
        icon: Eye,
        onclick: () => onDetail(agent),
      },
      {
        id: "logs",
        label: $t("dashboard.ctx_agent_logs"),
        icon: ScrollText,
        onclick: () => onLogs(agent.name),
      },
    ];
  }
</script>

<div class="bg-card border border-border rounded-xl overflow-hidden">
  <DashboardCardHeader
    title={$t("dashboard.card_agents_title")}
    count={agents.length}
    dense
  />
  {#if agents.length === 0}
    <div class="px-4 pb-4 text-body-xs text-muted-foreground/80">
      {$t("dashboard.agents_empty")}
    </div>
  {:else}
    <div class="flex flex-col px-2 pb-2 gap-1" use:listNavigation>
      {#each visible as agent (agent.name)}
        <ContextMenu items={rowMenu(agent)}>
          <ListRow
            variant="nav"
            align="center"
            onclick={() => onDetail(agent)}
            aria-label={$t("dashboard.agent_detail_aria", { values: { name: agent.name } })}
            data-testid="dashboard-agent-row-{agent.name}"
          >
            <div
              class="w-5 h-5 rounded-md inline-flex items-center justify-center shrink-0 bg-gradient-to-br from-primary to-secondary"
            >
              <Sparkles size={10} class="text-primary-foreground" />
            </div>
            <div class="flex-1 min-w-0">
              <span class="text-body-xs font-medium text-foreground truncate block">
                {agent.name}
              </span>
              <BuilderOnly>
                <span
                  class="text-caption font-mono text-muted-foreground/70 truncate block"
                  data-testid="dashboard-agent-tech-line"
                >
                  {techLine(agent)}
                </span>
              </BuilderOnly>
            </div>
            <StatusDot
              color={agent.runtime_status === "active"
                ? "hsl(var(--primary))"
                : "hsl(var(--warning))"}
              glow={agent.runtime_status === "active"}
            />
          </ListRow>
        </ContextMenu>
      {/each}
      {#if overflow > 0}
        <Badge size="sm" variant="neutral">+ {overflow}</Badge>
      {/if}
    </div>
  {/if}
</div>
