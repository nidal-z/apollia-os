<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AuditTrailEntry } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { Shield, CheckCircle2, XCircle, ChevronDown } from "lucide-svelte";

  const PAGE_SIZE = 50;

  let entries = $state<AuditTrailEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expandedRows = $state<Set<string>>(new Set());
  let hasMore = $state(false);
  let loadingMore = $state(false);

  let filterTool = $state<string>("all");
  let filterAgent = $state<string>("all");

  let uniqueTools = $derived([...new Set(entries.map((e) => e.tool_name))].sort());
  let uniqueAgents = $derived([...new Set(entries.map((e) => e.agent_name))].sort());

  let filteredEntries = $derived(
    entries.filter((e) => {
      if (filterTool !== "all" && e.tool_name !== filterTool) return false;
      if (filterAgent !== "all" && e.agent_name !== filterAgent) return false;
      return true;
    }),
  );

  /** Status of an audit entry — derived from exit_code + stderr presence.
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

  function formatTimestamp(iso: string): string {
    if (!iso) return "";
    return new Date(iso).toLocaleString();
  }

  function formatDuration(ms: number | null): string {
    if (ms === null || ms === undefined) return "—";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  async function loadEntries(): Promise<void> {
    try {
      const result: AuditTrailEntry[] = await invoke("get_tool_audit_trail", {
        limit: PAGE_SIZE,
      });
      entries = result;
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
      const nextLimit = entries.length + PAGE_SIZE;
      const result: AuditTrailEntry[] = await invoke("get_tool_audit_trail", {
        limit: nextLimit,
      });
      entries = result;
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
  <!-- Purpose banner — explains why audit trail matters, premium framing. -->
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
        <div class="font-mono text-[10px] tracking-[1.4px] text-muted-foreground/70 uppercase mb-1.5">
          {$t('observability.audit_kpi_entries')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">{stats.entries}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="font-mono text-[10px] tracking-[1.4px] text-muted-foreground/70 uppercase mb-1.5">
          {$t('observability.audit_kpi_tools')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">{stats.tools}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="font-mono text-[10px] tracking-[1.4px] text-muted-foreground/70 uppercase mb-1.5">
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
        <div class="font-mono text-[10px] tracking-[1.4px] text-muted-foreground/70 uppercase mb-1.5">
          {$t('observability.audit_kpi_avg_duration')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">
          {stats.avgMs > 0 ? formatDuration(Math.round(stats.avgMs)) : "—"}
        </div>
      </article>
    </div>

    <!-- Filters -->
    <div class="flex flex-wrap items-center gap-x-5 gap-y-3">
      <div class="flex items-center gap-2">
        <label for="filter-tool" class="text-[12px] text-muted-foreground">
          {$t('observability.tool_filter')}
        </label>
        <Select id="filter-tool" class="h-8 w-auto" bind:value={filterTool}>
          <option value="all">{$t('observability.all_tools')}</option>
          {#each uniqueTools as tool (tool)}
            <option value={tool}>{tool}</option>
          {/each}
        </Select>
      </div>

      <div class="flex items-center gap-2">
        <label for="filter-agent" class="text-[12px] text-muted-foreground">
          {$t('observability.agent_filter')}
        </label>
        <Select id="filter-agent" class="h-8 w-auto" bind:value={filterAgent}>
          <option value="all">{$t('observability.all_agents')}</option>
          {#each uniqueAgents as agentName (agentName)}
            <option value={agentName}>{agentName}</option>
          {/each}
        </Select>
      </div>
    </div>

    <!-- Table -->
    {#if filteredEntries.length === 0}
      <div
        class="glass-card glass-border rounded-xl flex flex-col items-center justify-center py-16"
        data-testid="audit-trail-empty"
      >
        <div class="rounded-full glass-inset p-4 mb-4">
          <Shield class="h-8 w-8 text-muted-foreground/60" />
        </div>
        <p class="text-[13px] text-muted-foreground">{$t('observability.empty_audit')}</p>
      </div>
    {:else}
      <div
        class="glass-card glass-border rounded-xl overflow-hidden"
        data-testid="audit-trail-table"
      >
        <div class="overflow-x-auto">
          <table class="w-full min-w-[720px] text-[13px]">
            <thead>
              <tr class="border-b border-border/40">
                <th
                  scope="col"
                  class="text-left px-5 py-3 font-mono text-[10px] tracking-[1.4px] uppercase text-muted-foreground/70 font-semibold"
                >
                  {$t('observability.table.timestamp')}
                </th>
                <th
                  scope="col"
                  class="text-left px-3 py-3 font-mono text-[10px] tracking-[1.4px] uppercase text-muted-foreground/70 font-semibold"
                >
                  {$t('observability.table.tool')}
                </th>
                <th
                  scope="col"
                  class="text-left px-3 py-3 font-mono text-[10px] tracking-[1.4px] uppercase text-muted-foreground/70 font-semibold"
                >
                  {$t('observability.table.agent')}
                </th>
                <th
                  scope="col"
                  class="text-right px-3 py-3 font-mono text-[10px] tracking-[1.4px] uppercase text-muted-foreground/70 font-semibold"
                >
                  {$t('observability.table.duration')}
                </th>
                <th
                  scope="col"
                  class="text-left px-3 py-3 font-mono text-[10px] tracking-[1.4px] uppercase text-muted-foreground/70 font-semibold"
                >
                  {$t('observability.table.status')}
                </th>
                <th
                  scope="col"
                  class="text-right px-5 py-3 font-mono text-[10px] tracking-[1.4px] uppercase text-muted-foreground/70 font-semibold w-8"
                  aria-label="expand"
                ></th>
              </tr>
            </thead>
            <tbody>
              {#each filteredEntries as entry (entry.id)}
                {@const status = entryStatus(entry)}
                {@const isExpanded = expandedRows.has(entry.id)}
                <tr
                  class="cursor-pointer border-b border-border/30 last:border-0 transition-colors hover:bg-muted/40"
                  class:bg-muted={isExpanded}
                  onclick={() => toggleRow(entry.id)}
                  data-testid="audit-row-{entry.id}"
                >
                  <td class="px-5 py-2.5 text-[11.5px] text-muted-foreground tabular-nums whitespace-nowrap">
                    {formatTimestamp(entry.timestamp)}
                  </td>
                  <td class="px-3 py-2.5">
                    <code class="font-mono text-[12px] text-foreground">{entry.tool_name}</code>
                  </td>
                  <td class="px-3 py-2.5 text-muted-foreground">{entry.agent_name}</td>
                  <td class="px-3 py-2.5 text-right tabular-nums">{formatDuration(entry.duration_ms)}</td>
                  <td class="px-3 py-2.5">
                    {#if status === "ok"}
                      <span class="inline-flex items-center gap-1.5 text-success">
                        <CheckCircle2 class="h-3.5 w-3.5" />
                        <span class="text-[11.5px] font-medium">{$t('observability.audit_status_ok')}</span>
                      </span>
                    {:else if status === "error"}
                      <span class="inline-flex items-center gap-1.5 text-destructive">
                        <XCircle class="h-3.5 w-3.5" />
                        <span class="text-[11.5px] font-medium">{$t('observability.audit_status_error')}</span>
                      </span>
                    {:else}
                      <span class="text-[11.5px] text-muted-foreground/60">
                        {$t('observability.audit_status_unknown')}
                      </span>
                    {/if}
                  </td>
                  <td class="px-5 py-2.5 text-right">
                    <ChevronDown
                      class="h-3.5 w-3.5 text-muted-foreground/60 transition-transform inline-block"
                      style={isExpanded ? "transform: rotate(180deg);" : ""}
                    />
                  </td>
                </tr>

                {#if isExpanded}
                  <tr class="bg-muted/20">
                    <td colspan="6" class="px-5 pb-4 pt-1">
                      <div class="space-y-3 rounded-lg glass-inset border border-border/30 p-4">
                        {#if entry.args_json}
                          <div>
                            <span class="font-mono text-[10px] tracking-[1.4px] uppercase text-muted-foreground/70 font-semibold">
                              {$t('observability.table.arguments')}
                            </span>
                            <pre
                              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-[11.5px] font-mono leading-relaxed"
                            >{entry.args_json}</pre>
                          </div>
                        {/if}
                        {#if entry.stdout}
                          <div>
                            <span class="font-mono text-[10px] tracking-[1.4px] uppercase text-muted-foreground/70 font-semibold">
                              stdout
                            </span>
                            <pre
                              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-[11.5px] font-mono leading-relaxed"
                            >{entry.stdout}</pre>
                          </div>
                        {/if}
                        {#if entry.stderr}
                          <div>
                            <span class="font-mono text-[10px] tracking-[1.4px] uppercase text-destructive font-semibold">
                              stderr
                            </span>
                            <pre
                              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-[11.5px] font-mono leading-relaxed text-destructive"
                            >{entry.stderr}</pre>
                          </div>
                        {/if}
                        {#if !entry.args_json && !entry.stdout && !entry.stderr}
                          <p class="text-[12px] text-muted-foreground italic">
                            {$t('observability.table.no_details')}
                          </p>
                        {/if}
                      </div>
                    </td>
                  </tr>
                {/if}
              {/each}
            </tbody>
          </table>
        </div>
      </div>

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
