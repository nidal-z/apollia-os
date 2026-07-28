<script lang="ts">
  /**
   * PackageAgentsTab - the agents bundled in a package. Each row links to the
   * matching installed agent (when present) and shows its role and live status.
   */
  import { t } from "svelte-i18n";
  import { Card, StatusDot } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { agents } from "$lib/stores/agents";
  import { isActive, statusColor } from "../agentStatus";
  import type { AgentPackageListItem } from "$lib/types";

  interface Props {
    pkg: AgentPackageListItem;
    onSelectAgent: (name: string) => void;
  }

  let { pkg, onSelectAgent }: Props = $props();
</script>

<Card class="max-w-3xl p-3.5">
  <div class="mb-2.5 flex items-center justify-between">
    <span class="text-body-xs font-semibold text-foreground">
      {$t("agents.tab_agents")} · {pkg.agents.length}
    </span>
  </div>
  {#if pkg.agents.length === 0}
    <div class="py-4 text-center text-caption text-muted-foreground">
      {$t("agents.package_no_agent")}
    </div>
  {:else}
    {#each pkg.agents as pa, i (pa.name)}
      {@const linked = $agents.find((x) => x.name === pa.name) ?? null}
      <div
        class="flex items-center gap-2.5 py-2 {i === pkg.agents.length - 1
          ? ''
          : 'border-b border-border/40'}"
      >
        <div class="min-w-0 flex-1">
          <Button
            variant="ghost"
            size="auto"
            type="button"
            onclick={() => linked && onSelectAgent(linked.name)}
            disabled={!linked}
            class="block w-full truncate p-0 text-left text-body-xs font-medium text-foreground hover:underline disabled:no-underline disabled:opacity-60"
          >
            {pa.name}
          </Button>
          <div class="mt-0.5 truncate text-caption text-muted-foreground">
            {pa.entry}
          </div>
        </div>
        <Badge size="sm" variant={pa.role === "director" ? "info" : "neutral"}>
          {pa.role}
        </Badge>
        {#if linked}
          <StatusDot color={statusColor(linked)} glow={isActive(linked)} size={5} />
        {/if}
      </div>
    {/each}
  {/if}
</Card>
