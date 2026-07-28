<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import type { AuditTrailEntry } from "$lib/types";
  import type { AuditRow } from "$lib/ipc/audit";
  import { getToolAuditTrail } from "$lib/ipc/audit";
  import { mapAuditRow } from "$lib/ipc/auditPlanRow";
  import { Button } from "$lib/components/ui/button";
  import { Shield } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { ErrorBanner, SkeletonList } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import AuditPurposeBanner from "./AuditPurposeBanner.svelte";
  import AuditStatsStrip from "./AuditStatsStrip.svelte";
  import AuditFilterBar from "./AuditFilterBar.svelte";
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
  let errState = $state<HumanizedError | null>(null);
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

  async function loadEntries(): Promise<void> {
    try {
      const result = await getToolAuditTrail(PAGE_SIZE);
      rows = result.map(mapAuditRow);
      hasMore = result.length >= PAGE_SIZE;
      errState = null;
    } catch (err: unknown) {
      errState = reportError(err, { surface: "inline" });
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
      // A failed "load more" keeps the existing rows and surfaces a transient
      // toast rather than replacing the table with the inline error banner.
      reportError(err, { surface: "toast" });
    } finally {
      loadingMore = false;
    }
  }

  onMount(() => {
    void loadEntries();
  });
</script>

<div class="space-y-5">
  <AuditPurposeBanner />

  {#if loading}
    <SkeletonList count={6} avatar={false} rowClass="py-1" />
  {:else if errState}
    <ErrorBanner
      message={errState.friendly_message}
      onretry={() => void loadEntries()}
      retryLabel={$t('common.retry')}
      data-testid="audit-error"
    />
  {:else}
    <AuditStatsStrip {stats} />

    <AuditFilterBar
      bind:filterTool
      bind:filterAgent
      {uniqueTools}
      {uniqueAgents}
      {runId}
    />

    <!-- Table -->
    {#if filteredRows.length === 0}
      <Card class="flex flex-col items-center justify-center py-16" data-testid="audit-trail-empty">
        <div class="rounded-full glass-inset p-4 mb-4">
          <Shield class="h-8 w-8 text-muted-foreground/60" />
        </div>
        <p class="text-body-sm text-muted-foreground">{$t('observability.empty_audit')}</p>
      </Card>
    {:else}
      <Card class="overflow-hidden" data-testid="audit-trail-table">
        <div class="overflow-x-auto">
          <table class="w-full min-w-[720px] text-body-sm">
            <thead>
              <tr class="border-b border-border/40">
                <th
                  scope="col"
                  class="section-meta text-left px-5 py-3"
                >
                  {$t('observability.table.timestamp')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-left px-3 py-3"
                >
                  {$t('observability.table.tool')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-left px-3 py-3"
                >
                  {$t('observability.table.agent')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-right px-3 py-3"
                >
                  {$t('observability.table.duration')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-left px-3 py-3"
                >
                  {$t('observability.table.status')}
                </th>
                <th
                  scope="col"
                  class="section-meta text-right px-5 py-3 w-8"
                  aria-label="expand"
                ></th>
              </tr>
            </thead>
            <tbody use:listNavigation={{ rowSelector: "tr[data-audit-row]", idPrefix: "audit-row" }}>
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
