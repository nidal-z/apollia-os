<script lang="ts">
  /**
   * PackageTriggersTab - the triggers declared by a package's agents, with their
   * source kind, config, and enabled state.
   */
  import { t } from "svelte-i18n";
  import { Zap } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { triggers } from "$lib/stores/sse";
  import type { AgentPackageListItem } from "$lib/types";

  interface Props {
    pkg: AgentPackageListItem;
  }

  let { pkg }: Props = $props();

  const names = $derived(new Set(pkg.agents.map((a) => a.name)));
  const rows = $derived($triggers.filter((tr) => names.has(tr.agent)));
  const enabledCount = $derived(rows.filter((tr) => tr.enabled).length);
</script>

<Card class="max-w-3xl p-3.5">
  <div class="mb-2.5 flex items-center justify-between">
    <span class="text-body-xs font-semibold text-foreground">
      {$t("agents.tab_triggers")} · {rows.length}
    </span>
    <span class="font-mono text-caption text-muted-foreground">
      {$t("agents.triggers_active_count", { values: { count: enabledCount } })}
    </span>
  </div>
  {#if rows.length === 0}
    <div class="py-4 text-center text-caption text-muted-foreground">
      {$t("agents.no_trigger_configured")}
    </div>
  {:else}
    {#each rows as tr, i (tr.id)}
      <div
        class="flex items-center gap-2.5 py-2 {i === rows.length - 1
          ? ''
          : 'border-b border-border/40'}"
      >
        <span
          class="inline-flex h-6 w-6 items-center justify-center rounded-md bg-primary/10 text-primary"
        >
          <Zap size={11} />
        </span>
        <div class="min-w-0 flex-1">
          <div class="truncate font-mono text-body-xs font-medium text-foreground">
            {tr.id}
          </div>
          <div class="truncate text-caption text-muted-foreground">
            {tr.agent} · {tr.source_kind}{tr.source_config ? ` · ${tr.source_config}` : ""}
          </div>
        </div>
        <Badge size="sm" variant={tr.enabled ? "success" : "neutral"}>
          {tr.enabled ? $t("agents.trigger_active") : $t("agents.trigger_inactive")}
        </Badge>
      </div>
    {/each}
  {/if}
</Card>
