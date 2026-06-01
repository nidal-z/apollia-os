<script lang="ts">
  /**
   * SheetContent - canonical scrollable body region for a `<Sheet>` panel.
   *
   * Applies the standard horizontal padding + comfortable vertical rhythm
   * and a flex-1 overflow-y-auto so the content scrolls while the header
   * and footer stay pinned.
   */
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  interface Props {
    /**
     * Padding profile :
     * - `default` = `px-6 py-5 space-y-4` (form panels, detail sheets).
     * - `compact` = `px-4 py-3 space-y-3` (dense log viewers).
     * - `flush`   = `px-0 py-0` (consumer manages padding entirely).
     */
    padding?: "default" | "compact" | "flush";
    /** Optional override class merged after the padding profile. */
    class?: string;
    /** Optional `data-testid` forwarded to the scroll container. */
    "data-testid"?: string;
    children: Snippet;
  }

  let {
    padding = "default",
    class: className = "",
    "data-testid": testid,
    children,
  }: Props = $props();

  const paddingClass = $derived(
    padding === "default"
      ? "px-6 py-5 space-y-4"
      : padding === "compact"
        ? "px-4 py-3 space-y-3"
        : "",
  );
</script>

<div class={cn("flex-1 overflow-y-auto", paddingClass, className)} data-testid={testid}>
  {@render children()}
</div>
