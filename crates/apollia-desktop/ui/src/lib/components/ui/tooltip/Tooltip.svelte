<script lang="ts">
  import { Tooltip as TooltipPrimitive } from "bits-ui";
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  interface Props {
    /** Tooltip body — plain string or a snippet for rich content. */
    content: string | Snippet;
    /** Preferred placement. */
    side?: "top" | "right" | "bottom" | "left";
    /** Open delay in ms (default 200). */
    delay?: number;
    /** Disable the tooltip entirely (still renders children). */
    disabled?: boolean;
    /** Extra classes merged into the content element. */
    class?: string;
    /** Trigger content (the element the tooltip anchors to). */
    children: Snippet;
  }

  let {
    content,
    side = "top",
    delay = 200,
    disabled = false,
    class: className = "",
    children,
  }: Props = $props();
</script>

{#if disabled}
  {@render children()}
{:else}
  <TooltipPrimitive.Root delayDuration={delay}>
    <TooltipPrimitive.Trigger class="inline-flex items-center">
      {@render children()}
    </TooltipPrimitive.Trigger>
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        {side}
        sideOffset={4}
        role="tooltip"
        style="z-index: var(--z-tooltip);"
        class={cn(
          "glass-card glass-border text-xs text-card-foreground shadow-md px-2.5 py-1.5 rounded-md",
          className,
        )}
      >
        {#if typeof content === "string"}
          {content}
        {:else}
          {@render content()}
        {/if}
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  </TooltipPrimitive.Root>
{/if}
