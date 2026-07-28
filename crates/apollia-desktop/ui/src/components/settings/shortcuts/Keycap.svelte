<script lang="ts">
  /**
   * Premium keycap chip - the single piece of expressive craft on the
   * otherwise calm Shortcuts page. Relief comes from a thicker bottom border
   * plus a subtle card-to-muted gradient; pressing it settles the cap by one
   * pixel. Modifiers are tinted primary. Colours are tokens only.
   */
  import { cn } from "$lib/utils";

  interface Props {
    label: string;
    variant?: "key" | "modifier";
    /** Widen the cap for word-length labels (Ctrl, Shift, Tab, Esc). */
    wide?: boolean;
    "data-testid"?: string;
  }

  let {
    label,
    variant = "key",
    wide = false,
    "data-testid": testid,
  }: Props = $props();
</script>

<kbd
  data-testid={testid}
  class={cn(
    "inline-flex h-7 min-w-7 select-none items-center justify-center rounded-md border border-b-2 border-border bg-gradient-to-b from-card to-muted font-mono text-caption font-semibold text-foreground transition-transform duration-75 ease-out active:translate-y-px active:border-b motion-reduce:transition-none",
    wide ? "px-2.5" : "px-2",
    variant === "modifier" &&
      "border-primary/30 bg-none bg-primary/10 text-primary",
  )}
>
  {label}
</kbd>
