<script lang="ts">
  import { cn } from "$lib/utils";
  import { scale, fade } from "svelte/transition";
  import { X } from "lucide-svelte";
  import { tick } from "svelte";

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

  // Mobile-first : largeur pleine jusqu'à sm (640 px), puis largeur plafonnée.
  // Seuil operator mobile = 375 px (voir src/lib/design/breakpoints.md).
  const sizeClasses: Record<string, string> = {
    sm: "w-full sm:max-w-[440px]",
    md: "w-full sm:max-w-[520px]",
    lg: "w-full sm:max-w-[620px]",
  };

  const FOCUSABLE_SELECTOR = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  let dialogContentRef = $state<HTMLDivElement | null>(null);
  let previouslyFocused: HTMLElement | null = null;
  let wasOpen = false;

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      onclose();
      return;
    }

    if (event.key === "Tab" && dialogContentRef) {
      const focusable = Array.from(
        dialogContentRef.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
      );
      if (focusable.length === 0) return;

      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
  }

  function handleDialogClick(event: MouseEvent) {
    if (event.target === event.currentTarget) {
      onclose();
    }
  }

  $effect(() => {
    if (open && !wasOpen) {
      previouslyFocused = document.activeElement as HTMLElement | null;
      tick().then(() => {
        if (dialogContentRef) {
          const first = dialogContentRef.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
          first?.focus();
        }
      });
    } else if (!open && wasOpen && previouslyFocused) {
      previouslyFocused.focus();
      previouslyFocused = null;
    }
    wasOpen = open;
  });
</script>

{#if open}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-50 bg-black/30 backdrop-blur-sm"
    role="presentation"
    transition:fade={{ duration: 200 }}
  ></div>

  <!-- Dialog -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    onkeydown={handleKeydown}
    onclick={handleDialogClick}
    transition:scale={{ start: 0.97, duration: 200 }}
    {...restProps}
  >
    <div
      bind:this={dialogContentRef}
      class={cn(
        "relative max-h-[85vh] overflow-y-auto rounded-lg border border-border bg-card text-card-foreground shadow-lg",
        sizeClasses[size],
        className,
      )}
      onclick={(e) => e.stopPropagation()}
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
