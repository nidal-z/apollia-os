<script lang="ts">
  /**
   * ConnectorSidebar - the split-screen sidebar for Connections.
   *
   * SidebarHeader (search + status FilterChipBar) over one list: every
   * connection the loader answers, whatever its kind, in its order, then the
   * built-in tools, the source of tools Apollia ships. Owns its
   * own search + status-filter state; selection is driven by the orchestrator.
   */
  import { t } from "svelte-i18n";
  import { Plus, Search, Wrench } from "lucide-svelte";
  import { SidebarHeader, FilterChipBar, ErrorBanner, ListRow } from "$lib/components/operator";
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
  import ConnectorTile from "../shared/ConnectorTile.svelte";
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

<SidebarHeader
  title={$t("connections.sidebar_my_connectors", { values: { count: rows.length } })}
>
  {#snippet search()}
    <div class="flex items-center gap-2 rounded-md border border-border bg-surface-1 px-2 py-1.5">
      <Search size={11} class="text-muted-foreground" />
      <Input
        type="search"
        unstyled
        bind:value={sidebarFilter}
        placeholder={$t("connections.sidebar_filter_placeholder")}
        aria-label={$t("connections.sidebar_filter_placeholder")}
        class="flex-1 text-body-xs text-foreground"
      />
    </div>
  {/snippet}
  {#snippet filters()}
    <FilterChipBar
      size="compact"
      chips={[
        { key: "all", label: $t("connections.status_chip_all"), color: "hsl(var(--muted-foreground))", count: rows.length },
        { key: "active", label: $t("connections.status_chip_active"), color: "hsl(var(--success))", count: countOf("active"), glowWhenActive: true },
        { key: "attention", label: $t("connections.status_chip_attention"), color: "hsl(var(--warning))", count: countOf("attention") },
        { key: "error", label: $t("connections.status_chip_error"), color: "hsl(var(--destructive))", count: countOf("error") },
        { key: "idle", label: $t("connections.status_chip_idle"), color: "hsl(var(--muted-foreground))", count: countOf("idle") },
      ]}
      activeKey={statusFilter}
      onchange={(key) => (statusFilter = key as StatusFilter)}
      aria-label={$t("connections.status_filter_label")}
      testidPrefix="connections-sidebar-filter"
    />
  {/snippet}
</SidebarHeader>

<div class="flex-1 overflow-auto px-2.5 pb-3" data-testid="connections-sidebar-list" use:listNavigation>
  {#if loadError}
    <ErrorBanner message={loadError} class="mx-1.5 my-2" data-testid="connections-error" />
  {/if}

  {#if loading && rows.length === 0}
    {#each Array(4) as _, i (i)}
      <div class="flex items-center gap-2.5 px-2.5 py-2.5">
        <Skeleton class="h-8 w-0.5 rounded-sm" />
        <div class="flex-1 space-y-1.5">
          <Skeleton class="h-3 w-3/5 rounded" />
          <Skeleton class="h-2 w-2/5 rounded" />
        </div>
      </div>
    {/each}
  {:else if visibleRows.length === 0 && !builtinVisible}
    <p class="px-2 py-3 text-caption text-muted-foreground/70" data-testid="connections-sidebar-empty">
      {rows.length === 0
        ? $t("connections.sidebar_no_servers")
        : sidebarFilter
          ? $t("connections.sidebar_no_results")
          : $t("connections.sidebar_no_servers_status")}
    </p>
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

  {#if builtinVisible}
    <section class="mt-3" data-testid="connections-builtin-section">
      <h3 class="px-2.5 pb-1 text-overline font-semibold uppercase text-muted-foreground">
        {$t("connections.builtin_section")}
      </h3>
      <ListRow
        variant="nav"
        state={builtinSelected ? "active" : "default"}
        align="stretch"
        class="text-left"
        onclick={() => onselect({ kind: "builtin" })}
        data-testid="connections-builtin-tools"
      >
        <ConnectorTile size="sm" accent="hsl(var(--primary))">
          {#snippet icon()}<Wrench size={13} />{/snippet}
        </ConnectorTile>
        <div class="min-w-0 flex-1">
          <span
            class="block truncate text-label-md text-foreground"
            style:font-weight={builtinSelected ? 600 : 500}
          >
            {builtinLabel}
          </span>
          <span class="mt-0.5 block truncate text-caption text-muted-foreground" data-testid="connections-builtin-count">
            {$t("connections.builtin_count", {
              values: { active: builtinTools.active, total: builtinTools.total },
            })}
          </span>
        </div>
      </ListRow>
    </section>
  {/if}

  <ForbiddenConnectionRows connections={forbidden} />
</div>

<div class="border-t border-border px-3 py-2">
  <Button
    variant="outline"
    size="sm"
    onclick={onopencatalogue}
    class="w-full justify-center"
    data-testid="connections-open-catalogue"
  >
    {#snippet icon()}<Plus size={12} />{/snippet}
    {$t("connections.add_connection")}
  </Button>
</div>
