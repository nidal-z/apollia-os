<script lang="ts">
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";
  import type { Icon } from "lucide-svelte";
  import type { HTMLTextareaAttributes } from "svelte/elements";

  interface Props extends HTMLTextareaAttributes {
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
          "pointer-events-none absolute left-3 top-3 text-muted-foreground",
          disabled && "opacity-40",
        )}
        aria-hidden="true"
      >
        <IconComponent size={16} aria-hidden="true" />
      </span>
    {/if}
    <textarea
      bind:value
      class={cn(
        "flex min-h-[80px] w-full rounded-md border border-border bg-background px-3 py-2 text-sm ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50 resize-y",
        hasIcon && "pl-10",
        hasTrailing && "pr-10",
        className,
      )}
      {disabled}
      {...restProps}
    ></textarea>
    {#if trailing}
      <span class={cn("absolute bottom-2 right-2 flex items-center", disabled && "opacity-40")}>
        {@render trailing()}
      </span>
    {/if}
  </div>
{:else}
  <textarea
    bind:value
    class={cn(
      "flex min-h-[80px] w-full rounded-md border border-border bg-background px-3 py-2 text-sm ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50 resize-y",
      className,
    )}
    {disabled}
    {...restProps}
  ></textarea>
{/if}
