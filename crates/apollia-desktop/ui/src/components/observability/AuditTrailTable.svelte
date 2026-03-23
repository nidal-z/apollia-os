<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AuditTrailEntry } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { Shield } from "lucide-svelte";

  const PAGE_SIZE = 50;

  let entries = $state<AuditTrailEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expandedRows = $state<Set<string>>(new Set());
  let hasMore = $state(false);
  let loadingMore = $state(false);

  let filterTool = $state<string>("all");
  let filterAgent = $state<string>("all");

  let uniqueTools = $derived(
    [...new Set(entries.map((e) => e.tool_name))].sort(),
  );

  let uniqueAgents = $derived(
    [...new Set(entries.map((e) => e.agent_name))].sort(),
  );

  let filteredEntries = $derived(
    entries.filter((e) => {
      if (filterTool !== "all" && e.tool_name !== filterTool) return false;
      if (filterAgent !== "all" && e.agent_name !== filterAgent) return false;
      return true;
    }),
  );

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
    if (ms === null) return "-";
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

<div class="space-y-4">
  <!-- Filters -->
  <div class="flex flex-wrap items-center gap-4">
    <div class="flex items-center gap-2">
      <label for="filter-tool" class="text-sm text-muted-foreground">{$t('observability.tool_filter')}</label>
      <Select
        id="filter-tool"
        class="h-8 w-auto"
        bind:value={filterTool}
      >
        <option value="all">{$t('observability.all_tools')}</option>
        {#each uniqueTools as tool (tool)}
          <option value={tool}>{tool}</option>
        {/each}
      </Select>
    </div>

    <div class="flex items-center gap-2">
      <label for="filter-agent" class="text-sm text-muted-foreground">{$t('observability.agent_filter')}</label>
      <Select
        id="filter-agent"
        class="h-8 w-auto"
        bind:value={filterAgent}
      >
        <option value="all">{$t('observability.all_agents')}</option>
        {#each uniqueAgents as agentName (agentName)}
          <option value={agentName}>{agentName}</option>
        {/each}
      </Select>
    </div>
  </div>

  <!-- Table -->
  {#if loading}
    <p class="text-sm text-muted-foreground">{$t('observability.loading_audit')}</p>
  {:else if error}
    <p class="text-sm text-destructive">{error}</p>
  {:else if filteredEntries.length === 0}
    <!-- AC-5 — Empty state -->
    <div class="glass-card glass-border flex flex-col items-center justify-center py-12" data-testid="audit-trail-empty">
      <Shield class="h-12 w-12 text-muted-foreground mb-4 opacity-50" />
      <p class="text-muted-foreground">{$t('observability.empty_audit')}</p>
    </div>
  {:else}
    <!-- AC-4 — Standard table styling -->
    <div class="glass-card glass-border rounded-lg overflow-hidden" data-testid="audit-trail-table">
      <table class="w-full text-[13px]">
        <thead class="border-b border-border bg-muted/50">
          <tr>
            <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('observability.table.timestamp')}</th>
            <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('observability.table.tool')}</th>
            <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('observability.table.agent')}</th>
            <th class="text-right px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('observability.table.duration')}</th>
            <th class="text-right px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('observability.table.exit')}</th>
          </tr>
        </thead>
        <tbody>
          {#each filteredEntries as entry (entry.id)}
            <tr
              class="cursor-pointer border-b border-border last:border-0 transition-colors hover:bg-muted"
              onclick={() => toggleRow(entry.id)}
            >
              <td class="px-3 py-2 text-[11px] text-muted-foreground">{formatTimestamp(entry.timestamp)}</td>
              <td class="px-3 py-2">{entry.tool_name}</td>
              <td class="px-3 py-2 text-muted-foreground">{entry.agent_name}</td>
              <td class="px-3 py-2 text-right">{formatDuration(entry.duration_ms)}</td>
              <td class="px-3 py-2 text-right">
                {#if entry.exit_code !== null}
                  <span class={entry.exit_code === 0 ? "text-emerald-600 dark:text-emerald-400" : "text-destructive"}>
                    {entry.exit_code}
                  </span>
                {:else}
                  <span class="text-muted-foreground">-</span>
                {/if}
              </td>
            </tr>

            {#if expandedRows.has(entry.id)}
              <tr>
                <td colspan="5" class="px-4 pb-3 pt-1">
                  <div class="space-y-2 rounded glass-border glass-inset p-3">
                    {#if entry.args_json}
                      <div>
                        <span class="text-xs font-medium text-muted-foreground">{$t('observability.table.arguments')}:</span>
                        <pre class="mt-1 overflow-x-auto rounded glass-surface p-2 text-xs">{entry.args_json}</pre>
                      </div>
                    {/if}
                    {#if entry.stdout}
                      <div>
                        <span class="text-xs font-medium text-muted-foreground">stdout:</span>
                        <pre class="mt-1 overflow-x-auto rounded glass-surface p-2 text-xs">{entry.stdout}</pre>
                      </div>
                    {/if}
                    {#if entry.stderr}
                      <div>
                        <span class="text-xs font-medium text-muted-foreground">stderr:</span>
                        <pre class="mt-1 overflow-x-auto rounded glass-surface p-2 text-xs text-destructive">{entry.stderr}</pre>
                      </div>
                    {/if}
                    {#if !entry.args_json && !entry.stdout && !entry.stderr}
                      <p class="text-xs text-muted-foreground">{$t('observability.table.no_details')}</p>
                    {/if}
                  </div>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>

    {#if hasMore}
      <div class="flex justify-center">
        <Button
          variant="outline"
          size="sm"
          disabled={loadingMore}
          onclick={() => void loadMore()}
        >
          {loadingMore ? $t('common.loading') : $t('observability.load_more_audit', { values: { count: PAGE_SIZE } })}
        </Button>
      </div>
    {/if}
  {/if}
</div>
