<script lang="ts" module>
  export interface ListColumn {
    label: string;
    /** Width/flex utility classes, e.g. "flex-[2] min-w-0", "w-[140px]", "w-[90px] text-right". */
    class?: string;
  }
</script>

<script lang="ts">
  /**
   * ListPanel - canonical bordered list/table container.
   *
   * Replaces the repeated `rounded-xl border border-border/60 bg-card
   * overflow-hidden` wrapper (Tasks, Inbox, Automations, Projects). Optionally
   * renders the standard uppercase column-header row from `columns`, or a custom
   * `header` snippet. Rows go in `children`; `footer` is optional.
   */
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  interface Props {
    /** Standard column header row. Ignored when `header` snippet is provided. */
    columns?: ListColumn[];
    header?: Snippet;
    footer?: Snippet;
    children: Snippet;
    class?: string;
    "data-testid"?: string;
  }

  let {
    columns,
    header,
    footer,
    children,
    class: className = "",
    "data-testid": testid,
  }: Props = $props();
</script>

<div
  class={cn("rounded-xl border border-border/60 bg-card overflow-hidden", className)}
  data-testid={testid}
>
  {#if header}
    {@render header()}
  {:else if columns && columns.length}
    <div
      class="px-4 py-2.5 border-b border-border/60 flex items-center gap-2.5 text-micro-lg uppercase tracking-[1px] font-semibold text-muted-foreground/70"
    >
      {#each columns as col}
        <div class={col.class}>{col.label}</div>
      {/each}
    </div>
  {/if}

  {@render children()}

  {#if footer}
    {@render footer()}
  {/if}
</div>
