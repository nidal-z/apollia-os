<script lang="ts">
  import { Accordion as AccordionPrimitive } from "bits-ui";
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  type Props =
    | {
        type: "single";
        value?: string;
        /** Present for API symmetry; single accordions are collapsible by default in bits-ui. */
        collapsible?: boolean;
        class?: string;
        children: Snippet;
      }
    | {
        type: "multiple";
        value?: string[];
        collapsible?: boolean;
        class?: string;
        children: Snippet;
      };

  let {
    type,
    value = $bindable(type === "single" ? "" : []) as any,
    class: className = "",
    children,
  }: Props = $props();
</script>

{#if type === "single"}
  <AccordionPrimitive.Root
    type="single"
    bind:value={value as string}
    class={cn("w-full", className)}
  >
    {@render children()}
  </AccordionPrimitive.Root>
{:else}
  <AccordionPrimitive.Root
    type="multiple"
    bind:value={value as string[]}
    class={cn("w-full", className)}
  >
    {@render children()}
  </AccordionPrimitive.Root>
{/if}
