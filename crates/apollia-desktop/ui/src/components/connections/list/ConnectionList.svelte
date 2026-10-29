<script lang="ts">
  /**
   * ConnectionList - the grid of the Connections page under its search: one
   * tile per connection the search, the kind chip and the state count let
   * through, what asks for an action first, then the entry of the tools
   * Apollia ships. With no connection at all, an invitation to add one and
   * three services of the selection to start with.
   */
  import { t, locale } from "svelte-i18n";
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
    type StateFilter,
  } from "$lib/connections/connectionRows";
  import { brandLogoPath } from "$lib/connections/brandLogo";
  import { enrichmentLabel } from "$lib/connections/catalogueText";
  import type { RepairKind } from "$lib/connections/connectionCard";
  import type { EmptySuggestion, TileFacts } from "$lib/connections/connectionDashboard";
  import ConnectionTile from "./ConnectionTile.svelte";
  import BrandLogo from "../shared/BrandLogo.svelte";
  import type { BuiltinToolsCount } from "../shared/types";

  interface Props {
    rows: ConnectionRow[];
    query: string;
    kind: KindFilter;
    /** The state count chosen in the summary. */
    state?: StateFilter;
    selection: RowSelection | null;
    loading: boolean;
    builtinTools: BuiltinToolsCount;
    /** The agents and last use of each tile, by connection identifier. */
    facts?: ReadonlyMap<string, TileFacts>;
    /** The services an empty page offers. */
    suggestions?: EmptySuggestion[];
    onselect: (selection: RowSelection) => void;
    onadd: () => void;
    /** Open the add window on the service `entryId` of the selection. */
    onsuggest?: (entryId: string) => void;
    /** Check the connection `id` now, from the badge of its tile. */
    oncheck?: (id: string) => void;
    /** Repair the connection `id` the way its tile offers. */
    onrepair?: (id: string, kind: RepairKind) => void;
    /** The connections being checked. */
    checking?: ReadonlySet<string>;
  }

  let {
    rows,
    query,
    kind,
    state = "all",
    selection,
    loading,
    builtinTools,
    facts,
    suggestions = [],
    onselect,
    onadd,
    onsuggest,
    oncheck,
    onrepair,
    checking,
  }: Props = $props();

  const shown = $derived(visibleRows(rows, query, kind, state));
  const builtinLabel = $derived($t("connections.builtin_name"));
  const builtinVisible = $derived(
    kind === "all" &&
      state === "all" &&
      (query.trim() === "" || builtinLabel.toLowerCase().includes(query.trim().toLowerCase())),
  );
</script>

{#if !loading && rows.length === 0}
  <section
    class="mx-auto flex max-w-xl flex-col items-center gap-4 px-3 py-10 text-center"
    data-testid="connections-sidebar-empty"
  >
    <h2 class="text-base font-semibold text-foreground">{$t("connections.empty.title")}</h2>
    <p class="text-body-sm text-muted-foreground">{$t("connections.empty.body")}</p>
    {#if suggestions.length > 0}
      <ul class="flex flex-wrap justify-center gap-2" aria-label={$t("connections.empty.suggestions")}>
        {#each suggestions as suggestion (suggestion.entryId)}
          {@const path = brandLogoPath(suggestion.enrichment.icon_name)}
          <li>
            <button
              type="button"
              class="inline-flex items-center gap-2 rounded-lg border border-border bg-card px-3 py-2 text-body-sm font-medium text-foreground transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              onclick={() => (onsuggest ? onsuggest(suggestion.entryId) : onadd())}
              data-testid="connections-empty-suggestion-{suggestion.entryId}"
            >
              {#if path}<BrandLogo {path} size="md" />{/if}
              {enrichmentLabel(suggestion.enrichment, $locale) ?? suggestion.entryId}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <Button variant="primary-solid" size="sm" onclick={onadd} data-testid="connections-empty-add">
      {#snippet icon()}<Plus size={12} />{/snippet}
      {$t("connections.add_connection")}
    </Button>
  </section>
{:else}
  <div class="grid grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-3" use:listNavigation>
    {#if loading && rows.length === 0}
      {#each Array(4) as _, i (i)}
        <div class="flex flex-col gap-2.5 rounded-lg border border-border p-3.5">
          <div class="flex items-center gap-3">
            <Skeleton class="h-8 w-8 rounded-lg" />
            <Skeleton class="h-3 w-2/5 rounded" />
          </div>
          <Skeleton class="h-2 w-1/3 rounded" />
          <Skeleton class="h-2 w-1/2 rounded" />
        </div>
      {/each}
    {:else if shown.length === 0}
      <p class="col-span-full px-3 py-8 text-center text-body-sm text-muted-foreground" data-testid="connections-grid-empty">
        {query.trim() ? $t("connections.sidebar_no_results") : $t("connections.list.no_connection_of_kind")}
      </p>
    {:else}
      {#each shown as row (row.key)}
        <ConnectionTile
          {row}
          facts={facts?.get(row.key)}
          selected={isSelected(row, selection)}
          onselect={() => onselect({ kind: "connection", id: row.key })}
          oncheck={oncheck ? () => oncheck(row.key) : undefined}
          onrepair={onrepair ? (repair) => onrepair(row.key, repair) : undefined}
          checking={checking?.has(row.key) ?? false}
        />
      {/each}
    {/if}
  </div>
{/if}

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
