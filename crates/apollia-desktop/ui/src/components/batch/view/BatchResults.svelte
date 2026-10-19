<script lang="ts">
  /**
   * BatchResults - the results of a batch as the launch asked for them: one
   * row per item, one column per field of its answer. A header sorts on its
   * field (amounts as numbers); the fields of few values are filters, each
   * value with how many answers give it. A cell the answer left empty, and an
   * answer that did not give its fields, are marked. A row opens its item.
   */
  import { locale, t } from "svelte-i18n";
  import { AlertTriangle, ArrowDown, ArrowUp } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import type { BatchItem, BatchOutput } from "$lib/ipc/batches";
  import { ITEM_TONE } from "./batchView";
  import {
    cellMark,
    facetsOf,
    filterRows,
    nextSort,
    resultRows,
    rowsToReview,
    sortRows,
    toggleFilter,
    type ResultFilter,
    type ResultSort,
  } from "./batchResults";

  interface Props {
    items: BatchItem[];
    outputs: BatchOutput[];
    onopen: (rank: number) => void;
  }

  let { items, outputs, onopen }: Props = $props();

  let sort = $state<ResultSort>({ field: null, direction: "asc" });
  let filter = $state<ResultFilter>({});

  const rows = $derived(resultRows(items, outputs));
  const facets = $derived(facetsOf(rows, outputs));
  const shown = $derived(sortRows(filterRows(rows, filter), sort, $locale ?? "fr"));
  const toReview = $derived(rowsToReview(rows, outputs));
  const filtered = $derived(Object.keys(filter).length > 0);
</script>

<section class="space-y-3" data-testid="batch-results">
  {#if facets.length > 0}
    <div class="flex flex-wrap items-center gap-x-5 gap-y-2" data-testid="batch-results-facets">
      {#each facets as facet (facet.field)}
        <div class="flex flex-wrap items-center gap-1.5">
          <span class="text-caption text-muted-foreground">{facet.field}</span>
          {#each facet.values as entry (entry.value)}
            {@const on = (filter[facet.field] ?? "").toLowerCase() === entry.value.toLowerCase()}
            <button
              type="button"
              class={on
                ? "rounded-full border border-primary bg-primary/10 px-2.5 py-0.5 text-caption text-foreground"
                : "rounded-full border border-border bg-card px-2.5 py-0.5 text-caption text-foreground hover:bg-muted"}
              aria-pressed={on}
              onclick={() => (filter = toggleFilter(filter, facet.field, entry.value))}
              data-testid="batch-results-facet"
            >
              {entry.value} <span class="tabular-nums text-muted-foreground">{entry.count}</span>
            </button>
          {/each}
        </div>
      {/each}
      {#if filtered}
        <button
          type="button"
          class="text-caption text-primary underline underline-offset-2"
          onclick={() => (filter = {})}
          data-testid="batch-results-clear"
        >
          {$t("batch_view.results.clear")}
        </button>
      {/if}
    </div>
  {/if}

  <p class="m-0 text-caption text-muted-foreground" data-testid="batch-results-count">
    {$t("batch_view.results.count", { values: { shown: shown.length, total: rows.length } })}
    {#if toReview > 0}
      <span class="ml-2 inline-flex items-center gap-1 text-warning-a11y">
        <AlertTriangle size={12} />
        {$t("batch_view.results.to_review", { values: { count: toReview } })}
      </span>
    {/if}
  </p>

  <div class="overflow-x-auto rounded-lg border border-border bg-surface-1">
    <table class="w-full border-collapse text-left text-body-sm" data-testid="batch-results-table">
      <thead class="sticky top-0 bg-surface-2">
        <tr>
          <th class="whitespace-nowrap border-b border-border px-3 py-2 text-caption font-medium text-muted-foreground">
            {$t("batch_view.results.item")}
          </th>
          {#each outputs as output (output.name)}
            {@const sorted = sort.field === output.name}
            <th
              class="border-b border-border px-3 py-2"
              title={output.description}
              aria-sort={sorted ? (sort.direction === "asc" ? "ascending" : "descending") : "none"}
            >
              <button
                type="button"
                class="inline-flex items-center gap-1 whitespace-nowrap text-caption font-medium text-muted-foreground hover:text-foreground"
                onclick={() => (sort = nextSort(sort, output.name))}
                data-testid="batch-results-sort"
              >
                {output.name}
                {#if sorted}
                  {#if sort.direction === "asc"}<ArrowUp size={12} />{:else}<ArrowDown size={12} />{/if}
                {/if}
              </button>
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each shown as row (row.rank)}
          <tr
            class="cursor-pointer border-b border-border/60 last:border-b-0 hover:bg-muted/50"
            onclick={() => onopen(row.rank)}
            data-testid="batch-results-row"
            data-rank={row.rank}
          >
            <td class="max-w-[16rem] px-3 py-2 align-top">
              <span class="block break-words font-medium text-foreground">{row.title}</span>
              {#if row.status !== "completed"}
                <Badge variant={ITEM_TONE[row.status]} outline>{$t(`batch_view.item_status.${row.status}`)}</Badge>
              {:else if row.unfitted}
                <span class="text-caption text-warning-a11y" data-testid="batch-results-unfitted">
                  {$t("batch_view.results.unfitted")}
                </span>
              {/if}
            </td>
            {#each outputs as output (output.name)}
              {@const value = row.cells[output.name]}
              <td
                class={cellMark(row, output.name) === "missing"
                  ? "px-3 py-2 align-top text-warning-a11y"
                  : "px-3 py-2 align-top text-foreground"}
                data-testid="batch-results-cell"
              >
                {#if value !== null}
                  <span class="line-clamp-4 break-words">{value}</span>
                {:else if cellMark(row, output.name) === "missing"}
                  <span class="text-caption">{$t("batch_view.results.missing")}</span>
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>
