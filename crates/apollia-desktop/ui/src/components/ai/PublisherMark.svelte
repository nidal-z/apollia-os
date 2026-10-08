<script lang="ts">
  /**
   * PublisherMark - the mark of a publisher beside its name: its initials on
   * a tile. No logo of a third party ships with the application, so the mark
   * is drawn from the name alone.
   */
  import { cn } from "$lib/utils";
  import { publisherInitials } from "./discover";

  interface Props {
    name: string;
    size?: "sm" | "md";
  }

  let { name, size = "md" }: Props = $props();
</script>

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
