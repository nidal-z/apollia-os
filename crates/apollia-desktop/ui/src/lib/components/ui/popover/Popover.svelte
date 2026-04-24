<script lang="ts">
  import { Popover as PopoverPrimitive } from "bits-ui";
  import { cn } from "$lib/utils";
  import { fade, scale, type TransitionConfig } from "svelte/transition";
  import { prefersReducedMotion } from "$lib/design/motion";
  import type { Snippet } from "svelte";

  interface Props {
    /** Controlled/bindable open state. */
    open?: boolean;
    /** Preferred placement relative to the trigger. */
    side?: "top" | "right" | "bottom" | "left";
    /** Alignment along the side. */
    align?: "start" | "center" | "end";
    /** Extra classes merged into the content element. */
    class?: string;
    /** Renders the trigger element. Receives bits-ui merged props to spread on your element. */
    trigger: Snippet<[Record<string, unknown>]>;
    /** Renders the floating popover content. */
    content: Snippet;
  }

  let {
    open = $bindable(false),
    side = "bottom",
    align = "center",
    class: className = "",
    trigger,
    content,
  }: Props = $props();

  function popoverTransition(node: Element): TransitionConfig {
    if (prefersReducedMotion()) {
      return fade(node, { duration: 120 });
    }
    return scale(node, { start: 0.96, duration: 200 });
  }
</script>

<PopoverPrimitive.Root bind:open>
  <PopoverPrimitive.Trigger>
    {#snippet child({ props })}
      {@render trigger(props)}
    {/snippet}
  </PopoverPrimitive.Trigger>
  <PopoverPrimitive.Portal>
    <PopoverPrimitive.Content
      {side}
      {align}
      sideOffset={4}
      style="z-index: var(--z-overlay);"
      class={cn(
        "glass-card glass-border rounded-lg shadow-lg p-2 min-w-[200px]",
        className,
      )}
      forceMount
    >
      {#snippet child({ wrapperProps, props, open: isOpen })}
        {#if isOpen}
          <div {...wrapperProps}>
            <div {...props} transition:popoverTransition>
              {@render content()}
            </div>
          </div>
        {/if}
      {/snippet}
    </PopoverPrimitive.Content>
  </PopoverPrimitive.Portal>
</PopoverPrimitive.Root>
