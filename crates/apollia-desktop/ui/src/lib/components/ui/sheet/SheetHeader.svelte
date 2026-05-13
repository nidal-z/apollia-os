<script lang="ts">
  /**
   * SheetHeader — canonical header bar for a `<Sheet>` panel.
   *
   * Structure : title (h2) + optional subtitle + close button + optional
   * trailing actions snippet. Mirrors PageHeader's ergonomics but at panel
   * scale (smaller padding, no kicker).
   */
  import { X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    subtitle?: string;
    /** Leading visual (icon, status pill, …) rendered before the title. */
    leading?: Snippet;
    /** Render custom content inside the title slot (replaces title/subtitle). */
    titleSlot?: Snippet;
    /** Right-aligned action area BEFORE the close button. */
    actions?: Snippet;
    /** Close handler — when provided, the × button is rendered. */
    onclose?: () => void;
    /** Accessible label for the close button. Defaults to "Close". */
    closeLabel?: string;
    /** Optional override of the heading id (for `aria-labelledby` on the sheet root). */
    titleId?: string;
    class?: string;
  }

  let {
    title,
    subtitle,
    leading,
    titleSlot,
    actions,
    onclose,
    closeLabel = "Close",
    titleId,
    class: className = "",
  }: Props = $props();
</script>

<header
  class={cn(
    "flex items-start justify-between gap-3 border-b border-border/60 px-6 py-4",
    className,
  )}
>
  {#if leading}
    <div class="flex shrink-0 items-center">{@render leading()}</div>
  {/if}
  <div class="min-w-0 flex-1">
    {#if titleSlot}
      {@render titleSlot()}
    {:else}
      <h2 id={titleId} class="m-0 truncate text-base font-semibold text-foreground">
        {title}
      </h2>
      {#if subtitle}
        <p class="mt-1 text-xs text-muted-foreground">{subtitle}</p>
      {/if}
    {/if}
  </div>
  <div class="flex shrink-0 items-center gap-1">
    {#if actions}
      {@render actions()}
    {/if}
    {#if onclose}
      <Button
        variant="ghost"
        size="icon-sm"
        onclick={onclose}
        aria-label={closeLabel}
        data-testid="sheet-close"
      >
        <X size={16} strokeWidth={1.75} />
      </Button>
    {/if}
  </div>
</header>
