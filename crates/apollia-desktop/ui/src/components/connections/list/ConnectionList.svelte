<script lang="ts">
  /**
   * ConnectionList - the column of the Connections page under its search: one
   * row per connection the search and the kind chip let through, what needs
   * repair first, then the entry of the tools Apollia ships.
   */
  import { t } from "svelte-i18n";
  import { ChevronRight, Plus, Wrench } from "lucide-svelte";
  import { ListRow } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import {
    isSelected,
    visibleRows,
    type ConnectionRow,
    type KindFilter,
    type RowSelection,
  } from "$lib/connections/connectionRows";
  import ConnectionListItem from "./ConnectionListItem.svelte";
  import type { BuiltinToolsCount } from "../shared/types";

  interface Props {
    rows: ConnectionRow[];
    query: string;
    kind: KindFilter;
    selection: RowSelection | null;
    loading: boolean;
    builtinTools: BuiltinToolsCount;
    onselect: (selection: RowSelection) => void;
    onadd: () => void;
    /** Check the connection `id` now, from the badge of its row. */
    oncheck?: (id: string) => void;
    /** The connections being checked. */
    checking?: ReadonlySet<string>;
  }

  let { rows, query, kind, selection, loading, builtinTools, onselect, onadd, oncheck, checking }: Props = $props();

  const shown = $derived(visibleRows(rows, query, kind));
  const builtinLabel = $derived($t("connections.builtin_name"));
  const builtinVisible = $derived(
    kind === "all" &&
      (query.trim() === "" || builtinLabel.toLowerCase().includes(query.trim().toLowerCase())),
  );
</script>

<div class="flex flex-col gap-0.5" use:listNavigation>
  {#if loading && rows.length === 0}
    {#each Array(4) as _, i (i)}
      <div class="flex items-center gap-3 px-3 py-2.5">
        <Skeleton class="h-8 w-8 rounded-lg" />
        <div class="flex-1 space-y-1.5">
          <Skeleton class="h-3 w-2/5 rounded" />
          <Skeleton class="h-2 w-1/4 rounded" />
        </div>
      </div>
    {/each}
  {:else if shown.length === 0}
    <div class="flex flex-col items-center gap-3 px-3 py-8 text-center" data-testid="connections-sidebar-empty">
      <p class="text-body-sm text-muted-foreground">
        {rows.length === 0
          ? $t("connections.sidebar_no_servers")
          : query.trim()
            ? $t("connections.sidebar_no_results")
            : $t("connections.list.no_connection_of_kind")}
      </p>
      {#if rows.length === 0}
        <Button variant="primary-solid" size="sm" onclick={onadd}>
          {#snippet icon()}<Plus size={12} />{/snippet}
          {$t("connections.add_connection")}
        </Button>
      {/if}
    </div>
  {:else}
    {#each shown as row (row.key)}
      <ConnectionListItem
        {row}
        selected={isSelected(row, selection)}
        onselect={() => onselect({ kind: "connection", id: row.key })}
        oncheck={oncheck ? () => oncheck(row.key) : undefined}
        checking={checking?.has(row.key) ?? false}
      />
    {/each}
  {/if}
</div>

{#if builtinVisible}
  <section class="mt-4 border-t border-border/60 pt-3" data-testid="connections-builtin-section">
    <h2 class="px-3 pb-1.5 text-overline uppercase text-muted-foreground">
      {$t("connections.builtin_section")}
    </h2>
    <ListRow
      variant="nav"
      state={selection?.kind === "builtin" ? "active" : "default"}
      align="center"
      class="gap-3 px-3 py-2.5 text-left"
      onclick={() => onselect({ kind: "builtin" })}
      data-testid="connections-builtin-tools"
    >
      <span class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground">
        <Wrench size={15} strokeWidth={1.75} aria-hidden="true" />
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
