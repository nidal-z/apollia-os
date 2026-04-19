<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    class?: string;
    children?: import("svelte").Snippet;
  }

  let { class: className = "", children }: Props = $props();
</script>

<div
  class={cn(
    "mt-6 flex flex-wrap justify-end gap-2 sm:gap-3",
    className,
  )}
>
  {@render children?.()}
</div>
