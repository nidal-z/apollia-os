<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    children?: Snippet;
    icon?: Snippet;
    kbd?: Snippet;
    onclick?: (e: MouseEvent) => void;
    disabled?: boolean;
    type?: "button" | "submit" | "reset";
  }

  let {
    children,
    icon,
    kbd,
    onclick,
    disabled = false,
    type = "button",
  }: Props = $props();
</script>

<button
  {type}
  {disabled}
  {onclick}
  class="inline-flex items-center gap-[7px] px-[13px] py-[7px] rounded-lg border-0 bg-primary text-white text-[12.5px] font-medium cursor-pointer shadow-[0_2px_6px_-1px_hsl(var(--primary)/0.35)] hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
>
  {#if icon}
    {@render icon()}
  {/if}
  {#if children}
    {@render children()}
  {/if}
  {#if kbd}
    {@render kbd()}
  {/if}
</button>
