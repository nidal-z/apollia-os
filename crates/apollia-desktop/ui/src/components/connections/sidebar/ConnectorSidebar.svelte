<script lang="ts">
  /**
   * ConnectorSidebar - the list of the Connections page.
   *
   * A line of filters (search + status FilterChipBar) over one list: every
   * connection the loader answers, whatever its kind, in its order, then the
   * built-in tools, the source of tools Apollia ships. Owns its
   * own search + status-filter state; selection is driven by the orchestrator.
   */
  import { t } from "svelte-i18n";
  import { ChevronRight, Plus, Search, Wrench } from "lucide-svelte";
  import { FilterChipBar, ErrorBanner, ListRow } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import {
    isSelected,
    rowMatches,
    type ConnectionRow,
    type RowSelection,
  } from "$lib/connections/connectionRows";
  import type { BuiltinToolsCount, StatusFilter, McpRowAction } from "../shared/types";
  import ConnectionListRow from "./ConnectionListRow.svelte";
  import ForbiddenConnectionRows from "./ForbiddenConnectionRows.svelte";
  import type { Connection } from "$lib/types";

  interface Props {
    rows: ConnectionRow[];
    /** Connections the organisation policy forbade at the last start. */
    forbidden?: Connection[];
    selection: RowSelection | null;
    loading: boolean;
    loadError: string | null;
    onselect: (sel: RowSelection) => void;
    onopencatalogue: () => void;
    oncontext: (name: string, action: McpRowAction) => void;
    builtinTools: BuiltinToolsCount;
  }

  let {
    rows,
    forbidden = [],
    selection,
    loading,
    loadError,
    onselect,
    onopencatalogue,
    oncontext,
    builtinTools,
  }: Props = $props();

  let sidebarFilter = $state("");
  let statusFilter = $state<StatusFilter>("all");

  const countOf = (status: StatusFilter): number => rows.filter((r) => r.status === status).length;

  const visibleRows = $derived(
    rows
      .filter((r) => statusFilter === "all" || r.status === statusFilter)
      .filter((r) => rowMatches(r, sidebarFilter)),
  );

  const builtinLabel = $derived($t("connections.builtin_name"));
  const builtinVisible = $derived(
    statusFilter === "all" &&
      (sidebarFilter.trim() === "" ||
        builtinLabel.toLowerCase().includes(sidebarFilter.trim().toLowerCase())),
  );
  const builtinSelected = $derived(selection?.kind === "builtin");
</script>

<!-- One page in the grammar of the settings: a line of filters, then a card
     per kind of thing, each row a connection with its state on the right. -->
<div class="flex flex-wrap items-center gap-3">
  <div class="w-full max-w-sm">
    <Input
      type="search"
      size="sm"
      icon={Search}
      bind:value={sidebarFilter}
      placeholder={$t("connections.sidebar_filter_placeholder")}
      aria-label={$t("connections.sidebar_filter_placeholder")}
    />
  </div>
  <FilterChipBar
    size="compact"
    chips={[
      { key: "all", label: $t("connections.status_chip_all"), color: "hsl(var(--muted-foreground))", count: rows.length },
      { key: "active", label: $t("connections.status_chip_active"), color: "hsl(var(--success))", count: countOf("active") },
      { key: "attention", label: $t("connections.status_chip_attention"), color: "hsl(var(--warning))", count: countOf("attention") },
      { key: "error", label: $t("connections.status_chip_error"), color: "hsl(var(--destructive))", count: countOf("error") },
      { key: "idle", label: $t("connections.status_chip_idle"), color: "hsl(var(--muted-foreground))", count: countOf("idle") },
    ]}
    activeKey={statusFilter}
    onchange={(key) => (statusFilter = key as StatusFilter)}
    aria-label={$t("connections.status_filter_label")}
    testidPrefix="connections-sidebar-filter"
  />
</div>

{#if loadError}
  <ErrorBanner message={loadError} data-testid="connections-error" />
{/if}

<section class="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
  <h2 class="border-b border-border/60 px-4 py-2.5 text-overline uppercase text-muted-foreground">
    {$t("connections.sidebar_my_connectors", { values: { count: rows.length } })}
  </h2>
  <div class="divide-y divide-border/60" data-testid="connections-sidebar-list" use:listNavigation>
    {#if loading && rows.length === 0}
      {#each Array(4) as _, i (i)}
        <div class="flex items-center gap-3 px-4 py-3">
          <Skeleton class="h-9 w-9 rounded-lg" />
          <div class="flex-1 space-y-1.5">
            <Skeleton class="h-3 w-2/5 rounded" />
            <Skeleton class="h-2 w-1/4 rounded" />
          </div>
        </div>
      {/each}
    {:else if visibleRows.length === 0}
      <div class="flex flex-col items-center gap-3 px-4 py-8 text-center" data-testid="connections-sidebar-empty">
        <p class="text-body-sm text-muted-foreground">
          {rows.length === 0
            ? $t("connections.sidebar_no_servers")
            : sidebarFilter
              ? $t("connections.sidebar_no_results")
              : $t("connections.sidebar_no_servers_status")}
        </p>
        {#if rows.length === 0}
          <Button variant="primary-solid" size="sm" onclick={onopencatalogue}>
            {#snippet icon()}<Plus size={12} />{/snippet}
            {$t("connections.add_connection")}
          </Button>
        {/if}
      </div>
    {:else}
      {#each visibleRows as row (row.key)}
        <ConnectionListRow
          {row}
          selected={isSelected(row, selection)}
          onselect={() => onselect(row.selection)}
          oncontext={row.selection.kind === "mcp"
            ? (action) => oncontext(row.selection.kind === "mcp" ? row.selection.name : row.key, action)
            : undefined}
        />
      {/each}
    {/if}
  </div>
</section>

{#if builtinVisible}
  <section class="overflow-hidden rounded-xl border border-border bg-card shadow-sm" data-testid="connections-builtin-section">
    <h2 class="border-b border-border/60 px-4 py-2.5 text-overline uppercase text-muted-foreground">
      {$t("connections.builtin_section")}
    </h2>
    <ListRow
      variant="nav"
      state={builtinSelected ? "active" : "default"}
      align="center"
      class="gap-3 !rounded-none px-4 py-3 text-left"
      onclick={() => onselect({ kind: "builtin" })}
      data-testid="connections-builtin-tools"
    >
      <span class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground">
        <Wrench size={16} strokeWidth={1.75} />
      </span>
      <div class="min-w-0 flex-1">
        <span class="block truncate text-body-sm font-medium text-foreground">{builtinLabel}</span>
        <span class="mt-0.5 block truncate text-caption text-muted-foreground" data-testid="connections-builtin-count">
          {$t("connections.builtin_count", {
            values: { active: builtinTools.active, total: builtinTools.total },
          })}
        </span>
      </div>
      <ChevronRight size={15} class="shrink-0 text-muted-foreground" aria-hidden="true" />
    </ListRow>
  </section>
{/if}

<ForbiddenConnectionRows connections={forbidden} />
