<script lang="ts">
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";
  import type { Icon } from "lucide-svelte";
  import type { HTMLInputAttributes } from "svelte/elements";

  interface Props extends HTMLInputAttributes {
    class?: string;
    value?: string;
    icon?: typeof Icon;
    trailing?: Snippet;
  }

  let {
    class: className = "",
    value = $bindable(""),
    icon: IconComponent,
    trailing,
    disabled,
    ...restProps
  }: Props = $props();

  const hasIcon = $derived(IconComponent !== undefined);
  const hasTrailing = $derived(trailing !== undefined);
  const wrapped = $derived(hasIcon || hasTrailing);
</script>

{#if wrapped}
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
    <input
      class={cn(
        "flex h-10 w-full rounded-md border border-border bg-background px-3 py-2 text-sm ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:border-primary disabled:cursor-not-allowed disabled:opacity-50",
        hasIcon && "pl-10",
        hasTrailing && "pr-10",
        className,
      )}
      bind:value
      {disabled}
      {...restProps}
    />
    {#if trailing}
      <span class={cn("absolute right-2 top-1/2 -translate-y-1/2 flex items-center", disabled && "opacity-40")}>
        {@render trailing()}
      </span>
    {/if}
  </div>
{:else}
  <input
    class={cn(
      "flex h-10 w-full rounded-md border border-border bg-background px-3 py-2 text-sm ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:border-primary disabled:cursor-not-allowed disabled:opacity-50",
      className,
    )}
    bind:value
    {disabled}
    {...restProps}
  />
{/if}
