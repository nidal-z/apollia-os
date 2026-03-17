<script lang="ts">
  import { cn } from "$lib/utils";

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
    class="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
    role="button"
    tabindex="-1"
    onclick={handleBackdropClick}
    onkeydown={handleKeydown}
  ></div>

  <!-- Panel -->
  <div
    class={cn(
      "fixed inset-y-0 right-0 z-50 flex w-[400px] flex-col glass-panel glass-border border-l shadow-lg transition-transform",
      className,
    )}
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    onkeydown={handleKeydown}
  >
    {@render children?.()}
  </div>
{/if}
