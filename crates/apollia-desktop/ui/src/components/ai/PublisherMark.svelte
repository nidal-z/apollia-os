<script lang="ts">
  /**
   * PublisherMark - the mark of a publisher beside its name: the picture of
   * its organisation when the machine could read it, its initials on a tile
   * otherwise. No picture of a third party ships with the application.
   */
  import { cn } from "$lib/utils";
  import { publisherInitials } from "./discover";

  interface Props {
    name: string;
    /** The picture of the publisher, as a `data:` address. */
    src?: string | undefined;
    size?: "sm" | "md";
  }

  let { name, src = undefined, size = "md" }: Props = $props();
</script>

{#if src}
  <img
    {src}
    alt=""
    class={cn(
      "shrink-0 select-none border border-border bg-card object-cover",
      size === "md" ? "h-7 w-7 rounded-md" : "h-5 w-5 rounded-full",
    )}
    data-testid="ai-discover-publisher-mark"
  />
{:else}
  <span
    class={cn(
      "flex shrink-0 select-none items-center justify-center border border-border bg-muted font-semibold text-foreground/80",
      size === "md" ? "h-7 w-7 rounded-md text-xs" : "h-5 w-5 rounded-full text-micro",
    )}
    aria-hidden="true"
    data-testid="ai-discover-publisher-mark"
  >
    {publisherInitials(name)}
  </span>
{/if}
