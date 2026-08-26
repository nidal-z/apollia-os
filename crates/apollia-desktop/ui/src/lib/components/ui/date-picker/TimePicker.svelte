<script lang="ts">
  import { cn } from "$lib/utils";
  import { Clock } from "lucide-svelte";
  import type { HTMLInputAttributes } from "svelte/elements";

  interface Props extends Omit<HTMLInputAttributes, "type" | "value"> {
    value?: string;
    disabled?: boolean;
    class?: string;
  }

  let {
    value = $bindable(""),
    disabled = false,
    class: className = "",
    ...restProps
  }: Props = $props();
</script>

<div class="relative">
  <span
    class={cn(
      "pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
      disabled && "opacity-40",
    )}
    aria-hidden="true"
  >
    <Clock size={16} />
  </span>
  <input
    type="time"
    bind:value
    {disabled}
    class={cn(
      "time-picker-native flex h-10 w-full rounded-md border border-border bg-background pl-10 pr-3 py-2 text-sm ring-offset-background transition-shadow duration-fast placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:border-primary disabled:cursor-not-allowed disabled:opacity-50",
      className,
    )}
    {...restProps}
  />
</div>

<style>
  :global(.dark) .time-picker-native {
    color-scheme: dark;
  }
</style>
