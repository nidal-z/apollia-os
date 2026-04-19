<script lang="ts">
  import { Accordion as AccordionPrimitive } from "bits-ui";
  import { ChevronDown } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import { cn } from "$lib/utils";
  import { prefersReducedMotion } from "$lib/design/motion";
  import type { Snippet } from "svelte";

  interface Props {
    value: string;
    disabled?: boolean;
    class?: string;
    trigger: Snippet;
    content: Snippet;
  }

  let {
    value,
    disabled = false,
    class: className = "",
    trigger,
    content,
  }: Props = $props();
</script>

<AccordionPrimitive.Item
  {value}
  {disabled}
  class={cn("border-b border-border last:border-0", className)}
>
  <AccordionPrimitive.Header>
    <AccordionPrimitive.Trigger
      class="group flex w-full items-center justify-between py-3 px-4 text-sm font-medium text-foreground transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring data-[disabled]:cursor-not-allowed data-[disabled]:opacity-50"
    >
      {@render trigger()}
      <ChevronDown
        size={16}
        strokeWidth={1.75}
        class="shrink-0 text-muted-foreground transition-transform duration-200 group-data-[state=open]:rotate-180"
        aria-hidden="true"
      />
    </AccordionPrimitive.Trigger>
  </AccordionPrimitive.Header>
  <AccordionPrimitive.Content forceMount>
    {#snippet child({ props, open })}
      {#if open}
        <div
          {...props}
          transition:slide={{ duration: prefersReducedMotion() ? 0 : 200 }}
        >
          <div class="px-4 pb-4 text-sm text-muted-foreground">
            {@render content()}
          </div>
        </div>
      {/if}
    {/snippet}
  </AccordionPrimitive.Content>
</AccordionPrimitive.Item>
