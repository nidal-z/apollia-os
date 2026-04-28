<script lang="ts">
  import type { Snippet } from "svelte";

  type Tone =
    | "neutral"
    | "primary"
    | "success"
    | "warning"
    | "danger"
    | "info"
    | "secondary";

  interface Props {
    children?: Snippet;
    icon?: Snippet;
    /** Semantic tone — controls bg + text color when no explicit override given. */
    tone?: Tone;
    size?: "sm" | "md";
    /** Use a transparent (outlined) background regardless of tone. */
    outline?: boolean;
  }

  let {
    children,
    icon,
    tone = "neutral",
    size = "md",
    outline = false,
  }: Props = $props();

  const TONE_CLASS: Record<Tone, { bg: string; fg: string }> = {
    neutral: { bg: "bg-surface-1", fg: "text-muted-foreground" },
    primary: { bg: "bg-primary/10", fg: "text-primary" },
    success: { bg: "bg-success/10", fg: "text-success-a11y" },
    warning: { bg: "bg-warning/10", fg: "text-warning-a11y" },
    danger: { bg: "bg-destructive/10", fg: "text-danger-a11y" },
    info: { bg: "bg-info/10", fg: "text-info" },
    secondary: { bg: "bg-secondary/10", fg: "text-secondary-foreground" },
  };

  const cls = $derived(TONE_CLASS[tone]);
</script>

<span
  class="inline-flex items-center gap-1.5 rounded-full font-medium {size === 'sm'
    ? 'text-[10px] px-[7px] py-[1px]'
    : 'text-[11px] px-[9px] py-[3px]'} {outline
    ? 'bg-transparent border border-border ' + cls.fg
    : cls.bg + ' ' + cls.fg + ' border-none'}"
>
  {#if icon}
    {@render icon()}
  {/if}
  {#if children}
    {@render children()}
  {/if}
</span>
