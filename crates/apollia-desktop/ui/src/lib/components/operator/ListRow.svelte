<script lang="ts">
  /**
   * ListRow - canonical clickable list-row shell.
   *
   * Owns the repeated row scaffolding only: state background (active / unread /
   * dimmed), padding profile, bottom hairline, hover, and the button role +
   * Enter-to-activate keyboard handling. The inner layout (columns, or
   * leading + content + trailing) is supplied by the caller via `children`, so
   * domain rows (TaskRow, InboxRow, ConversationRow, ...) stay thin wrappers and
   * render pixel-identically to before.
   */
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  type RowState = "default" | "active" | "unread" | "dimmed";

  interface Props {
    /** active = selected (bg-primary/10), unread = bg-primary/5, dimmed = 0.55 opacity. */
    state?: RowState;
    /** Independent 0.55 opacity (e.g. closed/archived), composable with `state`. */
    dim?: boolean;
    /** Cross-axis alignment of the row's direct children. `stretch` matches the bare flex default. */
    align?: "start" | "center" | "stretch";
    /** Padding profile: default = px-4 py-3, snug = px-3.5 py-3. */
    pad?: "default" | "snug";
    /**
     * Visual variant:
     * - `row` (default) - full-width row with an optional bottom hairline.
     * - `nav` - rounded pill for sidebar navigation lists (no hairline, the
     *   state background is rounded). Padding tightens to px-2.5 py-2.
     */
    variant?: "row" | "nav";
    /** Bottom hairline divider (ignored for the `nav` variant). */
    border?: boolean;
    /** When provided, the row becomes an interactive button. */
    onclick?: (e: MouseEvent | KeyboardEvent) => void;
    /** Override the default Enter-to-activate handler. */
    onkeydown?: (e: KeyboardEvent) => void;
    class?: string;
    children: Snippet;
    "data-testid"?: string;
    "aria-label"?: string;
  }

  let {
    state = "default",
    dim = false,
    align = "start",
    pad = "default",
    variant = "row",
    border = true,
    onclick,
    onkeydown,
    class: className = "",
    children,
    "data-testid": testid,
    "aria-label": ariaLabel,
  }: Props = $props();

  const isNav = $derived(variant === "nav");

  const bgClass = $derived(
    state === "active"
      ? "bg-primary/10"
      : state === "unread"
        ? "bg-primary/5"
        : "bg-transparent hover:bg-muted/40",
  );

  const padClass = $derived(
    isNav ? "px-2.5 py-2" : pad === "snug" ? "px-3.5 py-3" : "px-4 py-3",
  );

  function handleKeydown(e: KeyboardEvent): void {
    if (onkeydown) {
      onkeydown(e);
      return;
    }
    if (onclick && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      onclick(e);
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  role={onclick ? "button" : undefined}
  tabindex={onclick ? 0 : undefined}
  onclick={onclick}
  onkeydown={onclick || onkeydown ? handleKeydown : undefined}
  data-testid={testid}
  aria-label={ariaLabel}
  class={cn(
    "group relative flex gap-2.5 transition-colors",
    padClass,
    align === "center" ? "items-center" : align === "stretch" ? "items-stretch" : "items-start",
    isNav ? "rounded-lg" : border && "border-b border-border/60",
    onclick && "cursor-pointer",
    bgClass,
    className,
  )}
  style:opacity={dim || state === "dimmed" ? "0.55" : undefined}
>
  {@render children()}
</div>
