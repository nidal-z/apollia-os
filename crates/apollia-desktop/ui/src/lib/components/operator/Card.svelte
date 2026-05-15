<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    children: Snippet;
    /** Apply hover-lift micro-interaction. */
    hover?: boolean;
    /**
     * Optional primary action — when provided, the card becomes a focusable,
     * keyboard-activatable element (`role="button"`, Enter / Space to fire).
     * Caller is responsible for `stopPropagation()` on inner buttons.
     */
    onclick?: (e: MouseEvent | KeyboardEvent) => void;
    /** Accessibility label used when `onclick` is set. */
    ariaLabel?: string;
    /** Forwarded to `data-testid` on the wrapper. */
    "data-testid"?: string;
    class?: string;
  }

  let {
    children,
    hover = false,
    onclick,
    ariaLabel,
    "data-testid": dataTestid,
    class: className = "",
  }: Props = $props();

  function handleKeydown(e: KeyboardEvent): void {
    if (!onclick) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onclick(e);
    }
  }
</script>

<div
  class="bg-card border border-border rounded-xl transition-all {hover
    ? 'hover-lift'
    : ''} {onclick
    ? 'cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60'
    : ''} {className}"
  role={onclick ? "button" : undefined}
  tabindex={onclick ? 0 : undefined}
  aria-label={onclick ? ariaLabel : undefined}
  onclick={onclick}
  onkeydown={onclick ? handleKeydown : undefined}
  data-testid={dataTestid}
>
  {@render children()}
</div>
