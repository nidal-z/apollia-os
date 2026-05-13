<script lang="ts">
  import { cn } from "$lib/utils";
  import { scale, fade, type TransitionConfig } from "svelte/transition";
  import { backOut } from "svelte/easing";
  import { X } from "lucide-svelte";
  import { tick } from "svelte";
  import { t } from "svelte-i18n";
  import { prefersReducedMotion } from "$lib/design/motion";

  interface Props {
    open: boolean;
    onclose: () => void;
    size?: "sm" | "md" | "lg" | "xl";
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

  // `w-[min(Xpx,90vw)]` keeps the panel ≤ viewport on small screens while
  // locking a comfortable max width on desktop.
  const sizeClasses: Record<NonNullable<Props["size"]>, string> = {
    sm: "w-[min(440px,90vw)]",
    md: "w-[min(560px,90vw)]",
    lg: "w-[min(720px,90vw)]",
    xl: "w-[min(920px,90vw)]",
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

  // Spring scale entrance — swaps to plain fade when the user requests
  // reduced motion (§ prefers-reduced-motion).
  function dialogTransition(node: Element): TransitionConfig {
    if (prefersReducedMotion()) {
      return fade(node, { duration: 300 });
    }
    return scale(node, { start: 0.97, duration: 300, easing: backOut });
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
    class="fixed inset-0 backdrop-warm"
    style="z-index: var(--z-backdrop);"
    role="presentation"
    transition:fade={{ duration: 300 }}
  ></div>

  <!-- Dialog -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 flex items-center justify-center p-4"
    style="z-index: var(--z-overlay);"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    onkeydown={handleKeydown}
    onclick={handleDialogClick}
    transition:dialogTransition
    {...restProps}
  >
    <div
      bind:this={dialogContentRef}
      class={cn(
        "relative max-h-[90vh] overflow-y-auto rounded-lg shadow-lg",
        "bg-background text-foreground",
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
            aria-label={$t("a11y.close")}
            data-testid="dialog-close"
          >
            <X size={16} />
          </button>
        </div>
      {:else}
        <button
          class="absolute right-4 top-4 rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors z-10"
          onclick={onclose}
          aria-label={$t("a11y.close")}
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
