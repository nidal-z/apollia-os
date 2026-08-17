<script lang="ts" generics="T">
  import { ChevronUp, ChevronDown } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import type { Column, SortDirection } from "./types";
  import { t } from "svelte-i18n";

  interface Props {
    data: T[];
    columns: Array<Column<T>>;
    rowKey: (row: T) => string;
    onrowclick?: (row: T) => void;
    loading?: boolean;
    /** Text shown when `data` is empty. Falls back to `common.no_data`. */
    emptyLabel?: string;
    class?: string;
  }

  let {
    data,
    columns,
    rowKey,
    onrowclick,
    loading = false,
    emptyLabel,
    class: className = "",
  }: Props = $props();

  let sortKey = $state<string | null>(null);
  let sortDir = $state<SortDirection>("asc");

  function toggleSort(col: Column<T>) {
    if (!col.sortable) return;
    if (sortKey === col.key) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortKey = col.key;
      sortDir = "asc";
    }
  }

  const sortedData = $derived.by(() => {
    if (!sortKey) return data;
    const key = sortKey as keyof T;
    const dir = sortDir === "asc" ? 1 : -1;
    return [...data].sort((a, b) => {
      const av = a[key];
      const bv = b[key];
      if (av == null && bv == null) return 0;
      if (av == null) return -1 * dir;
      if (bv == null) return 1 * dir;
      if (typeof av === "number" && typeof bv === "number") {
        return (av - bv) * dir;
      }
      return String(av).localeCompare(String(bv)) * dir;
    });
  });

  const HIDE_ON_CLASS: Record<NonNullable<Column<T>["hideOn"]>, string> = {
    sm: "hidden sm:table-cell",
    md: "hidden md:table-cell",
    lg: "hidden lg:table-cell",
    xl: "hidden xl:table-cell",
  };

  function alignClass(align: Column<T>["align"]): string {
    if (align === "right") return "text-right";
    if (align === "center") return "text-center";
    return "text-left";
  }
</script>

<div class={cn("rounded-xl glass-card glass-border overflow-x-auto", className)} data-testid="data-table">
  <table class="w-full text-sm">
    <thead>
      <tr class="border-b border-border/50 text-xs text-muted-foreground/70 uppercase tracking-wider">
        {#each columns as col (col.key)}
          {@const hideCls = col.hideOn ? HIDE_ON_CLASS[col.hideOn] : ""}
          {@const alignCls = alignClass(col.align)}
          <th
            scope="col"
            class={cn(
              "px-4 py-3 font-medium whitespace-nowrap",
              alignCls,
              hideCls,
            )}
            style={col.width ? `width: ${col.width};` : undefined}
            aria-sort={col.sortable && sortKey === col.key
              ? sortDir === "asc" ? "ascending" : "descending"
              : col.sortable ? "none" : undefined}
          >
            {#if col.sortable}
              <button
                type="button"
                class="inline-flex items-center gap-1 font-medium uppercase tracking-wider text-muted-foreground/70 hover:text-foreground transition-colors"
                onclick={() => toggleSort(col)}
                data-testid="data-table-sort-{col.key}"
              >
                <span>{col.header}</span>
                {#if sortKey === col.key}
                  {#if sortDir === "asc"}
                    <ChevronUp size={12} strokeWidth={2} aria-hidden="true" />
                  {:else}
                    <ChevronDown size={12} strokeWidth={2} aria-hidden="true" />
                  {/if}
                {:else}
                  <ChevronUp size={12} strokeWidth={2} class="opacity-30" aria-hidden="true" />
                {/if}
              </button>
            {:else}
              <span>{col.header}</span>
            {/if}
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#if loading}
        {#each { length: 3 } as _, rowIdx (rowIdx)}
          <tr class="border-b border-border/20">
            {#each columns as col (col.key)}
              {@const hideCls = col.hideOn ? HIDE_ON_CLASS[col.hideOn] : ""}
              <td class={cn("px-4 py-3", hideCls)}>
                <Skeleton variant="text" />
              </td>
            {/each}
          </tr>
        {/each}
      {:else if sortedData.length === 0}
        <tr>
          <td
            colspan={columns.length}
            class="px-4 py-10 text-center text-sm text-muted-foreground"
            data-testid="data-table-empty"
          >
            {emptyLabel ?? $t("common.no_data")}
          </td>
        </tr>
      {:else}
        {#each sortedData as row (rowKey(row))}
          <tr
            class={cn(
              "border-b border-border/20 transition-colors hover:bg-muted/40",
              onrowclick ? "cursor-pointer" : "",
            )}
            onclick={onrowclick ? () => onrowclick(row) : undefined}
            data-testid="data-table-row"
          >
            {#each columns as col (col.key)}
              {@const hideCls = col.hideOn ? HIDE_ON_CLASS[col.hideOn] : ""}
              {@const alignCls = alignClass(col.align)}
              <td class={cn("px-4 py-3", alignCls, hideCls)}>
                {#if col.cell}
                  {@render col.cell(row)}
                {:else}
                  <span class="text-foreground">{row[col.key] as unknown as string}</span>
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      {/if}
    </tbody>
  </table>
</div>
