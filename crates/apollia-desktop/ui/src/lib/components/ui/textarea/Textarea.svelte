<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLTextareaAttributes } from "svelte/elements";

  interface Props extends HTMLTextareaAttributes {
    class?: string;
    value?: string;
  }

  let { class: className = "", value = $bindable(""), ...restProps }: Props = $props();
</script>

<textarea
  bind:value
  class={cn(
    "flex min-h-[80px] w-full rounded-md border border-border bg-background px-3 py-2 text-sm ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50 resize-y",
    className,
  )}
  {...restProps}
></textarea>
