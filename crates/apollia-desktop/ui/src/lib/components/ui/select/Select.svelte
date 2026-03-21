<script lang="ts">
  import { cn } from "$lib/utils";
  import { ChevronDown } from "lucide-svelte";
  import type { HTMLSelectAttributes } from "svelte/elements";

  interface Props extends HTMLSelectAttributes {
    class?: string;
    value?: string;
  }

  let { class: className = "", children, value = $bindable(""), ...restProps }: Props = $props();
</script>

<div class="relative">
  <select
    bind:value
    class={cn(
      "flex h-10 w-full appearance-none rounded-md border border-border bg-background px-3 py-2 pr-8 text-sm ring-offset-background transition-shadow duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50",
      className,
    )}
    {...restProps}
  >
    {@render children?.()}
  </select>
  <div class="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-muted-foreground">
    <ChevronDown size={14} strokeWidth={1.75} />
  </div>
</div>
