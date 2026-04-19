<script lang="ts">
  import { cn } from "$lib/utils";
  import { fly, fade } from "svelte/transition";

  interface Props {
    open: boolean;
    onclose: () => void;
    width?: "sm" | "md" | "lg";
    side?: "left" | "right";
    class?: string;
    children?: import("svelte").Snippet;
  }

  let {
    open,
    onclose,
    width = "md",
    side = "right",
    class: className = "",
    children,
  }: Props = $props();

  // Pixel widths mirror Dialog scale (US-SP42-012). `min(…, 90vw)` keeps
  // the panel from overflowing narrow viewports.
  const widthClasses: Record<NonNullable<Props["width"]>, string> = {
    sm: "w-full md:w-[min(360px,90vw)]",
    md: "w-full md:w-[min(480px,90vw)]",
    lg: "w-full md:w-[min(640px,90vw)]",
  };

  const widthPx: Record<NonNullable<Props["width"]>, number> = {
    sm: 360,
    md: 480,
    lg: 640,
  };

  const sidePosition = side === "left" ? "left-0 border-r" : "right-0 border-l";
  const flyX = side === "left" ? -widthPx[width] : widthPx[width];
  const easeOutCubic = (t: number) => 1 - Math.pow(1 - t, 3);

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
    class="fixed inset-0 backdrop-warm"
    style="z-index: var(--z-backdrop);"
    role="button"
    tabindex="-1"
    onclick={handleBackdropClick}
    onkeydown={handleKeydown}
    transition:fade={{ duration: 300 }}
  ></div>

  <!-- Panel -->
  <div
    class={cn(
      "fixed inset-y-0 flex flex-col glass-panel shadow-lg border-border",
      sidePosition,
      widthClasses[width],
      className,
    )}
    style="z-index: var(--z-overlay);"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    onkeydown={handleKeydown}
    transition:fly={{ x: flyX, duration: 300, easing: easeOutCubic }}
  >
    {@render children?.()}
  </div>
{/if}
