<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { AuditTrailEntry } from "$lib/types";
  import { Button } from "$lib/components/ui/button";

  const PAGE_SIZE = 50;

  let entries = $state<AuditTrailEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expandedRows = $state<Set<string>>(new Set());
  let hasMore = $state(false);
  let loadingMore = $state(false);

  /** Filter state. */
  let filterTool = $state<string>("all");
  let filterAgent = $state<string>("all");

  /** Unique tool names from loaded entries. */
  let uniqueTools = $derived(
    [...new Set(entries.map((e) => e.tool_name))].sort(),
  );

  /** Unique agent names from loaded entries (for the filter dropdown). */
  let uniqueAgents = $derived(
    [...new Set(entries.map((e) => e.agent_name))].sort(),
  );

  /** Filtered entries based on dropdown selections. */
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
      <label for="filter-tool" class="text-sm text-muted-foreground">Tool:</label>
      <select
        id="filter-tool"
        class="rounded-md border bg-background px-2 py-1 text-sm"
        bind:value={filterTool}
      >
        <option value="all">All tools</option>
        {#each uniqueTools as tool (tool)}
          <option value={tool}>{tool}</option>
        {/each}
      </select>
    </div>

    <div class="flex items-center gap-2">
      <label for="filter-agent" class="text-sm text-muted-foreground">Agent:</label>
      <select
        id="filter-agent"
        class="rounded-md border bg-background px-2 py-1 text-sm"
        bind:value={filterAgent}
      >
        <option value="all">All agents</option>
        {#each uniqueAgents as agentName (agentName)}
          <option value={agentName}>{agentName}</option>
        {/each}
      </select>
    </div>
  </div>

  <!-- Table -->
  {#if loading}
    <p class="text-sm text-muted-foreground">Loading audit trail...</p>
  {:else if error}
    <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
  {:else if filteredEntries.length === 0}
    <div class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12">
      <p class="text-muted-foreground">Aucune invocation d'outil enregistrée.</p>
    </div>
  {:else}
    <div class="overflow-x-auto">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b text-left text-xs text-muted-foreground">
            <th class="pb-2 pr-4 font-medium">Timestamp</th>
            <th class="pb-2 pr-4 font-medium">Tool</th>
            <th class="pb-2 pr-4 font-medium">Agent</th>
            <th class="pb-2 pr-4 text-right font-medium">Duration</th>
            <th class="pb-2 text-right font-medium">Exit</th>
          </tr>
        </thead>
        <tbody>
          {#each filteredEntries as entry (entry.id)}
            <!-- Main row -->
            <tr
              class="cursor-pointer border-b border-dashed transition-colors hover:bg-accent/30"
              onclick={() => toggleRow(entry.id)}
            >
              <td class="py-2 pr-4 text-xs">{formatTimestamp(entry.timestamp)}</td>
              <td class="py-2 pr-4">{entry.tool_name}</td>
              <td class="py-2 pr-4 text-muted-foreground">{entry.agent_name}</td>
              <td class="py-2 pr-4 text-right">{formatDuration(entry.duration_ms)}</td>
              <td class="py-2 text-right">
                {#if entry.exit_code !== null}
                  <span class={entry.exit_code === 0 ? "text-[var(--apollia-success)]" : "text-[hsl(var(--destructive))]"}>
                    {entry.exit_code}
                  </span>
                {:else}
                  <span class="text-muted-foreground">-</span>
                {/if}
              </td>
            </tr>

            <!-- Expanded detail -->
            {#if expandedRows.has(entry.id)}
              <tr>
                <td colspan="5" class="px-4 pb-3 pt-1">
                  <div class="space-y-2 rounded border bg-muted/20 p-3">
                    {#if entry.args_json}
                      <div>
                        <span class="text-xs font-medium text-muted-foreground">Arguments:</span>
                        <pre class="mt-1 overflow-x-auto rounded bg-muted/30 p-2 text-xs">{entry.args_json}</pre>
                      </div>
                    {/if}
                    {#if entry.stdout}
                      <div>
                        <span class="text-xs font-medium text-muted-foreground">stdout:</span>
                        <pre class="mt-1 overflow-x-auto rounded bg-muted/30 p-2 text-xs">{entry.stdout}</pre>
                      </div>
                    {/if}
                    {#if entry.stderr}
                      <div>
                        <span class="text-xs font-medium text-muted-foreground">stderr:</span>
                        <pre class="mt-1 overflow-x-auto rounded bg-muted/30 p-2 text-xs text-[hsl(var(--destructive))]">{entry.stderr}</pre>
                      </div>
                    {/if}
                    {#if !entry.args_json && !entry.stdout && !entry.stderr}
                      <p class="text-xs text-muted-foreground">No detailed data available.</p>
                    {/if}
                  </div>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>

    <!-- Load more -->
    {#if hasMore}
      <div class="flex justify-center">
        <Button
          variant="outline"
          size="sm"
          disabled={loadingMore}
          onclick={() => void loadMore()}
        >
          {loadingMore ? "Loading..." : "Load 50 more"}
        </Button>
      </div>
    {/if}
  {/if}
</div>
