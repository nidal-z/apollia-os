<script module lang="ts">
  import { cn } from "$lib/utils";

  /**
   * The classes of the list column: across the whole width when nothing is
   * open, a narrow column with a rule on its right when a detail card is.
   */
  export function browseListClass(detailOpen: boolean): string {
    return cn(
      "min-h-0 overflow-auto py-4",
      detailOpen ? "w-[360px] shrink-0 border-r border-border px-3" : "flex-1 px-6",
    );
  }
</script>

<script lang="ts">
  /**
   * BrowseLayout - the shell of a screen where one searches a list and opens
   * one entry: a header across the whole width (title, search, filters), an
   * offline banner closing that header, then the list, which narrows to a
   * column when the detail card of the chosen entry is shown beside it.
   *
   * It knows nothing of what is listed: the header, the list and the detail
   * are snippets, and the offline banner takes its words from the caller.
   */
  import type { Snippet } from "svelte";
  import { WifiOff } from "lucide-svelte";

  interface Props {
    /** A detail card is shown beside the list, which then keeps a narrow column. */
    detailOpen: boolean;
    /** The words of the offline banner; null or absent shows no banner. */
    offlineMessage?: string | null;
    /** What spans the width above the list: title, search, filters. */
    header: Snippet;
    /** The list itself, inside the scrolling column. */
    children: Snippet;
    /** The detail card of the chosen entry, rendered beside the list. */
    detail?: Snippet;
    "data-testid"?: string;
    /** data-testid on the scrolling list column. */
    listTestid?: string;
    /** data-testid on the offline banner. */
    offlineTestid?: string;
  }

  let {
    detailOpen,
    offlineMessage = null,
    header,
    children,
    detail,
    "data-testid": testid,
    listTestid,
    offlineTestid,
  }: Props = $props();
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col" data-testid={testid}>
  <header class="flex flex-col gap-4 border-b border-border px-6 pb-4 pt-5">
    {@render header()}

    {#if offlineMessage}
      <p
        class="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/5 p-2 text-xs text-foreground"
        data-testid={offlineTestid}
      >
        <WifiOff size={13} class="mt-0.5 shrink-0 text-warning-a11y" aria-hidden="true" />
        <span>{offlineMessage}</span>
      </p>
    {/if}
  </header>

  <div class="flex min-h-0 flex-1">
    <div class={browseListClass(detailOpen)} data-testid={listTestid}>
      {@render children()}
    </div>
    {@render detail?.()}
  </div>
</div>
