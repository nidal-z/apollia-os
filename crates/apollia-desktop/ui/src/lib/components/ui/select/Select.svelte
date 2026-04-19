<script lang="ts">
  import { cn } from "$lib/utils";
  import { ChevronDown, type Icon } from "lucide-svelte";
  import type { HTMLSelectAttributes } from "svelte/elements";

  interface Props extends HTMLSelectAttributes {
    class?: string;
    value?: string;
    icon?: typeof Icon;
  }

  let {
    class: className = "",
    children,
    value = $bindable(""),
    icon: IconComponent,
    disabled,
    ...restProps
  }: Props = $props();

  const hasIcon = $derived(IconComponent !== undefined);
</script>

<div class="relative">
  {#if IconComponent}
    <span
      class={cn(
        "pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
        disabled && "opacity-40",
      )}
      aria-hidden="true"
    >
      <IconComponent size={16} aria-hidden="true" />
    </span>
  {/if}
  <select
    bind:value
    class={cn(
      "flex h-10 w-full appearance-none rounded-md border border-border bg-background px-3 py-2 pr-8 text-sm ring-offset-background transition-shadow duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50",
      hasIcon && "pl-9",
      className,
    )}
    {disabled}
    {...restProps}
  >
    {@render children?.()}
  </select>
  <div
    class={cn(
      "pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-muted-foreground",
      disabled && "opacity-40",
    )}
  >
    <ChevronDown size={14} strokeWidth={1.75} />
  </div>
</div>
