<script lang="ts">
  import { Tooltip as TooltipPrimitive } from "bits-ui";
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  interface Props {
    /** Tooltip body - plain string or a snippet for rich content. */
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
    children?: Snippet;
    /**
     * Renders the trigger element itself, for a trigger that is already a
     * control (a button with its own click). Receives the bits-ui props to
     * spread on that element. Takes the place of `children`.
     */
    trigger?: Snippet<[Record<string, unknown>]>;
    /**
     * Makes the trigger itself the action: the trigger is already a button,
     * so an icon button that explains itself on hover passes its click here
     * rather than nesting a second button inside it.
     */
    onclick?: (event: MouseEvent) => void;
    /** Accessible name of the trigger, for an action trigger. */
    triggerLabel?: string;
    /** `data-testid` of the trigger, for an action trigger. */
    triggerTestid?: string;
    /** Extra classes on the trigger, for an action trigger. */
    triggerClass?: string;
  }

  let {
    content,
    side = "top",
    delay = 200,
    disabled = false,
    class: className = "",
    children,
    trigger,
    onclick,
    triggerLabel,
    triggerTestid,
    triggerClass = "",
  }: Props = $props();
</script>

{#if disabled}
  {#if trigger}{@render trigger({})}{:else}{@render children?.()}{/if}
{:else}
  <TooltipPrimitive.Root delayDuration={delay}>
    {#if trigger}
      <TooltipPrimitive.Trigger>
        {#snippet child({ props })}
          {@render trigger(props)}
        {/snippet}
      </TooltipPrimitive.Trigger>
    {:else}
      <TooltipPrimitive.Trigger
        class={cn("inline-flex items-center", triggerClass)}
        {onclick}
        aria-label={triggerLabel}
        data-testid={triggerTestid}
      >
        {@render children?.()}
      </TooltipPrimitive.Trigger>
    {/if}
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
