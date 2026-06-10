<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import type { AuditTrailEntry } from "$lib/types";
  import type { AuditRow } from "$lib/ipc/audit";
  import { getToolAuditTrail } from "$lib/ipc/audit";
  import { mapAuditRow } from "$lib/ipc/auditPlanRow";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { Shield } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { FormField } from "$lib/components/ui/form-field";
  import AuditVerifyButton from "./AuditVerifyButton.svelte";
  import PlanMutationRow from "./PlanMutationRow.svelte";
  import ToolAuditRow from "./ToolAuditRow.svelte";

  interface Props {
    /**
     * Run whose journal integrity can be verified from the action bar. When
     * omitted, the verify button is hidden (run selection is out of scope in
     * this version: the parent passes the current or last known run).
     */
    runId?: string | undefined;
  }

  let { runId = undefined }: Props = $props();

  const PAGE_SIZE = 50;

  let rows = $state<AuditRow[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expandedRows = $state<Set<string>>(new Set());
  let hasMore = $state(false);
  let loadingMore = $state(false);

  let filterTool = $state<string>("all");
  let filterAgent = $state<string>("all");

  let toolEntries = $derived(
    rows.filter((r) => r.type === "tool").map((r) => r.entry),
  );

  let uniqueTools = $derived([...new Set(toolEntries.map((e) => e.tool_name))].sort());
  let uniqueAgents = $derived([...new Set(toolEntries.map((e) => e.agent_name))].sort());

  // Tool rows honour the type/agent filters; plan-mutation rows are always
  // shown in chronological position (the type filters do not apply to them).
  let filteredRows = $derived(
    rows.filter((r) => {
      if (r.type !== "tool") return true;
      if (filterTool !== "all" && r.entry.tool_name !== filterTool) return false;
      if (filterAgent !== "all" && r.entry.agent_name !== filterAgent) return false;
      return true;
    }),
  );

  let filteredEntries = $derived(
    filteredRows.filter((r) => r.type === "tool").map((r) => r.entry),
  );

  /** Status of an audit entry - derived from exit_code + stderr presence.
   *  An MCP tool with no exit_code is considered "ok" unless stderr is set. */
  type EntryStatus = "ok" | "error" | "unknown";

  function entryStatus(e: AuditTrailEntry): EntryStatus {
    if (e.exit_code !== null && e.exit_code !== undefined) {
      return e.exit_code === 0 ? "ok" : "error";
    }
    if (e.stderr && e.stderr.trim().length > 0) return "error";
    if (e.duration_ms !== null && e.duration_ms !== undefined) return "ok";
    return "unknown";
  }

  let stats = $derived.by(() => {
    const visible = filteredEntries;
    const tools = new Set(visible.map((e) => e.tool_name)).size;
    let failures = 0;
    let totalDuration = 0;
    let durationSamples = 0;
    for (const e of visible) {
      if (entryStatus(e) === "error") failures += 1;
      if (e.duration_ms !== null && e.duration_ms !== undefined) {
        totalDuration += e.duration_ms;
        durationSamples += 1;
      }
    }
    const avgMs = durationSamples > 0 ? totalDuration / durationSamples : 0;
    return { entries: visible.length, tools, failures, avgMs };
  });

  function toggleRow(id: string) {
    const next = new Set(expandedRows);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    expandedRows = next;
  }

  function formatDuration(ms: number | null): string {
    if (ms === null || ms === undefined) return "-";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  async function loadEntries(): Promise<void> {
    try {
      const result = await getToolAuditTrail(PAGE_SIZE);
      rows = result.map(mapAuditRow);
      hasMore = result.length >= PAGE_SIZE;
      error = null;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function loadMore(): Promise<void> {
    loadingMore = true;
    try {
      const nextLimit = rows.length + PAGE_SIZE;
      const result = await getToolAuditTrail(nextLimit);
      rows = result.map(mapAuditRow);
      hasMore = result.length >= nextLimit;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingMore = false;
    }
  }

  onMount(() => {
    void loadEntries();
  });
</script>

<div class="space-y-5">
  <!-- Purpose banner - explains why audit trail matters, premium framing. -->
  <aside
    class="glass-card glass-border rounded-xl px-5 py-4 flex items-start gap-3"
    data-testid="audit-purpose"
  >
    <div class="rounded-lg glass-inset p-2 flex-shrink-0 mt-0.5">
      <Shield class="h-4 w-4 text-primary" aria-hidden="true" />
    </div>
    <div class="min-w-0">
      <h3 class="m-0 text-[13px] font-semibold tracking-[-0.1px] mb-1">
        {$t('observability.audit_purpose_title')}
      </h3>
      <p class="m-0 text-[12.5px] leading-[1.55] text-muted-foreground max-w-[720px]">
        {$t('observability.audit_purpose_body')}
      </p>
    </div>
  </aside>

  {#if loading}
    <p class="text-sm text-muted-foreground">{$t('observability.loading_audit')}</p>
  {:else if error}
    <p class="text-sm text-destructive">{error}</p>
  {:else}
    <!-- KPI strip -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-3" data-testid="audit-stats">
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
          {$t('observability.audit_kpi_entries')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">{stats.entries}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
          {$t('observability.audit_kpi_tools')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">{stats.tools}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
          {$t('observability.audit_kpi_failures')}
        </div>
        <div
          class="text-[20px] font-semibold tabular-nums leading-none"
          class:text-destructive={stats.failures > 0}
        >
          {stats.failures}
        </div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
          {$t('observability.audit_kpi_avg_duration')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">
          {stats.avgMs > 0 ? formatDuration(Math.round(stats.avgMs)) : "-"}
        </div>
      </article>
    </div>

    <!-- Filters -->
    <div class="flex flex-wrap items-center gap-x-5 gap-y-3">
      <FormField inline id="filter-tool" label={$t('observability.tool_filter')} labelClass="text-[12px] font-normal">
        <Select id="filter-tool" size="sm" class="w-auto" bind:value={filterTool}>
          <option value="all">{$t('observability.all_tools')}</option>
          {#each uniqueTools as tool (tool)}
            <option value={tool}>{tool}</option>
          {/each}
        </Select>
      </FormField>

      <FormField inline id="filter-agent" label={$t('observability.agent_filter')} labelClass="text-[12px] font-normal">
        <Select id="filter-agent" size="sm" class="w-auto" bind:value={filterAgent}>
          <option value="all">{$t('observability.all_agents')}</option>
          {#each uniqueAgents as agentName (agentName)}
            <option value={agentName}>{agentName}</option>
          {/each}
        </Select>
      </FormField>

      {#if runId}
        <div class="ml-auto">
          <AuditVerifyButton {runId} />
        </div>
      {/if}
    </div>

    <!-- Table -->
    {#if filteredRows.length === 0}
      <Card class="flex flex-col items-center justify-center py-16" data-testid="audit-trail-empty">
        <div class="rounded-full glass-inset p-4 mb-4">
          <Shield class="h-8 w-8 text-muted-foreground/60" />
        </div>
        <p class="text-[13px] text-muted-foreground">{$t('observability.empty_audit')}</p>
      </Card>
    {:else}
      <Card class="overflow-hidden" data-testid="audit-trail-table">
        <div class="overflow-x-auto">
          <table class="w-full min-w-[720px] text-[13px]">
            <thead>
              <tr class="border-b border-border/40">
                <th
                  scope="col"
                  class="section-meta text-left px-5 py-3 text-[10px] tracking-[1.4px]"
                >
                  {$t('observability.table.timestamp')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-left px-3 py-3 text-[10px] tracking-[1.4px]"
                >
                  {$t('observability.table.tool')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-left px-3 py-3 text-[10px] tracking-[1.4px]"
                >
                  {$t('observability.table.agent')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-right px-3 py-3 text-[10px] tracking-[1.4px]"
                >
                  {$t('observability.table.duration')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-left px-3 py-3 text-[10px] tracking-[1.4px]"
                >
                  {$t('observability.table.status')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-right px-5 py-3 text-[10px] tracking-[1.4px] w-8"
                  aria-label="expand"
                ></th>
              </tr>
            </thead>
            <tbody>
              {#each filteredRows as row (row.type === "tool" ? `tool:${row.entry.id}` : `plan:${row.entry.ordinal}`)}
                {#if row.type === "plan_mutation"}
                  <PlanMutationRow entry={row.entry} />
                {:else}
                  <ToolAuditRow
                    entry={row.entry}
                    expanded={expandedRows.has(row.entry.id)}
                    ontoggle={toggleRow}
                  />
                {/if}
              {/each}
            </tbody>
          </table>
        </div>
      </Card>

      {#if hasMore}
        <div class="flex justify-center">
          <Button
            variant="outline"
            size="sm"
            disabled={loadingMore}
            onclick={() => void loadMore()}
          >
            {loadingMore
              ? $t('common.loading')
              : $t('observability.load_more_audit', { values: { count: PAGE_SIZE } })}
          </Button>
        </div>
      {/if}
    {/if}
  {/if}
</div>
