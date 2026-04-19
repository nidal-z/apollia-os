<script lang="ts">
  /**
   * Icon — thin wrapper that applies a canonical tone to a lucide icon.
   *
   * Replaces inline `class="text-gray-500"` / `text-muted-foreground` /
   * `text-destructive` drifting across the codebase (F.72). The consumer
   * passes the icon component — this wrapper only owns the tone, size
   * and accessible label.
   */
  import { cn } from "$lib/utils";
  import type { Component } from "svelte";

  type Tone = "default" | "muted" | "primary" | "success" | "warning" | "danger";

  interface Props {
    icon: Component<{ size?: number | string; class?: string; [key: string]: unknown }>;
    tone?: Tone;
    size?: number;
    class?: string;
    /** Accessible label — omit when the icon is purely decorative. */
    label?: string;
  }

  let {
    icon: IconComponent,
    tone = "default",
    size = 16,
    class: className = "",
    label,
  }: Props = $props();

  const toneClass = $derived(({
    default: "text-foreground",
    muted: "text-muted-a11y",
    primary: "text-primary",
    success: "text-success-a11y",
    warning: "text-warning-a11y",
    danger: "text-danger-a11y",
  } as const)[tone]);
</script>

<span
  class={cn("inline-flex shrink-0", toneClass, className)}
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : true}
>
  <IconComponent {size} aria-hidden="true" />
</span>
