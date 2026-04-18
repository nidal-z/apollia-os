<script lang="ts">
  import { cn } from "$lib/utils";
  import { fly, fade } from "svelte/transition";

  interface Props {
    open: boolean;
    onclose: () => void;
    class?: string;
    children?: import("svelte").Snippet;
  }

  let { open, onclose, class: className = "", children }: Props = $props();

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      onclose();
    }
  }

  function handleBackdropClick() {
    onclose();
  }
</script>

{#if open}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-50 bg-black/30 backdrop-blur-sm"
    role="button"
    tabindex="-1"
    onclick={handleBackdropClick}
    onkeydown={handleKeydown}
    transition:fade={{ duration: 200 }}
  ></div>

  <!-- Panel : full-width sur mobile, largeur 400 px à partir de sm (voir breakpoints.md). -->
  <div
    class={cn(
      "fixed inset-y-0 right-0 z-50 flex w-full sm:max-w-[400px] flex-col glass-panel border-l border-border shadow-lg",
      className,
    )}
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    onkeydown={handleKeydown}
    transition:fly={{ x: 400, duration: 250, easing: (t) => 1 - Math.pow(1 - t, 3) }}
  >
    {@render children?.()}
  </div>
{/if}
