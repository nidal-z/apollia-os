<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    children?: Snippet;
    icon?: Snippet;
    onclick?: (e: MouseEvent) => void;
    disabled?: boolean;
    type?: "button" | "submit" | "reset";
  }

  let {
    children,
    icon,
    onclick,
    disabled = false,
    type = "button",
  }: Props = $props();
</script>

<button
  {type}
  {disabled}
  {onclick}
  class="inline-flex items-center gap-1.5 px-3 py-[7px] rounded-lg bg-surface-1 text-muted-foreground border border-border text-[12.5px] font-medium cursor-pointer hover:bg-surface-2 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
>
  {#if icon}
    {@render icon()}
  {/if}
  {#if children}
    {@render children()}
  {/if}
</button>
