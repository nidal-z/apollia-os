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
    /** Bottom hairline divider. */
    border?: boolean;
    /** When provided, the row becomes an interactive button. */
    onclick?: (e: MouseEvent) => void;
    /** Override the default Enter-to-activate handler. */
    onkeydown?: (e: KeyboardEvent) => void;
    class?: string;
    children: Snippet;
    "data-testid"?: string;
  }

  let {
    state = "default",
    dim = false,
    align = "start",
    pad = "default",
    border = true,
    onclick,
    onkeydown,
    class: className = "",
    children,
    "data-testid": testid,
  }: Props = $props();

  const bgClass = $derived(
    state === "active"
      ? "bg-primary/10"
      : state === "unread"
        ? "bg-primary/5"
        : "bg-transparent hover:bg-muted/40",
  );

  function handleKeydown(e: KeyboardEvent): void {
    if (onkeydown) {
      onkeydown(e);
      return;
    }
    if (onclick && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      onclick(e as unknown as MouseEvent);
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
  class={cn(
    "group relative flex gap-2.5 transition-colors",
    pad === "snug" ? "px-3.5 py-3" : "px-4 py-3",
    align === "center" ? "items-center" : align === "stretch" ? "items-stretch" : "items-start",
    border && "border-b border-border/60",
    onclick && "cursor-pointer",
    bgClass,
    className,
  )}
  style:opacity={dim || state === "dimmed" ? "0.55" : undefined}
>
  {@render children()}
</div>
