<script lang="ts">
  import { cn } from "$lib/utils";
  import { scale, fade } from "svelte/transition";
  import { X } from "lucide-svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    size?: "sm" | "md" | "lg";
    title?: string;
    class?: string;
    children?: import("svelte").Snippet;
    "data-testid"?: string;
  }

  let {
    open,
    onclose,
    size = "md",
    title,
    class: className = "",
    children,
    ...restProps
  }: Props = $props();

  const sizeClasses: Record<string, string> = {
    sm: "w-[440px]",
    md: "w-[520px]",
    lg: "w-[620px]",
  };

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

  <!-- Dialog -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    onkeydown={handleKeydown}
    transition:scale={{ start: 0.97, duration: 200 }}
    {...restProps}
  >
    <div
      class={cn(
        "relative max-h-[85vh] overflow-y-auto rounded-lg border border-border bg-card text-card-foreground shadow-lg",
        sizeClasses[size],
        className,
      )}
    >
      {#if title}
        <div class="flex items-center justify-between border-b border-border px-6 py-4">
          <h2 class="text-lg font-medium">{title}</h2>
          <button
            class="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            onclick={onclose}
            aria-label="Close"
            data-testid="dialog-close"
          >
            <X size={16} />
          </button>
        </div>
      {:else}
        <button
          class="absolute right-4 top-4 rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors z-10"
          onclick={onclose}
          aria-label="Close"
          data-testid="dialog-close"
        >
          <X size={16} />
        </button>
      {/if}
      <div class="p-6">
        {@render children?.()}
      </div>
    </div>
  </div>
{/if}
