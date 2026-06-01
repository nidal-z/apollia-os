<script lang="ts">
  /**
   * SheetFooter - canonical sticky footer bar for a `<Sheet>` panel.
   *
   * Mirrors `DialogFooter` but for Sheet contexts : right-aligned action
   * buttons row, top border, comfortable padding. Stays pinned at the
   * bottom of the panel while `SheetContent` scrolls above.
   */
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  interface Props {
    /** Optional override class. */
    class?: string;
    children: Snippet;
  }

  let { class: className = "", children }: Props = $props();
</script>

<footer
  class={cn(
    "flex shrink-0 items-center justify-end gap-2 border-t border-border/60 px-6 py-3",
    className,
  )}
>
  {@render children()}
</footer>
