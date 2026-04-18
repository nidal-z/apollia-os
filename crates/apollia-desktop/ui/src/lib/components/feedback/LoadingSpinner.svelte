<script lang="ts">
  /**
   * LoadingSpinner — SVG ring spinner tinted with primary by default.
   *
   * Prefer this over inline `animate-spin` + lucide icons so that loading
   * states stay consistent and obey `prefers-reduced-motion`.
   */
  import { cn } from "$lib/utils";

  interface Props {
    class?: string;
    /** Pixel size of the spinner square. */
    size?: number;
    /** Stroke width in px (relative to the fixed 24 viewBox). */
    strokeWidth?: number;
    /** `currentColor` picks up the surrounding text color. */
    tone?: "primary" | "current" | "muted";
    /** Accessible label — omit when the spinner is decorative. */
    label?: string;
  }

  let {
    class: className = "",
    size = 18,
    strokeWidth = 2.25,
    tone = "primary",
    label,
  }: Props = $props();

  const toneClass = $derived(({
    primary: "text-primary",
    current: "text-current",
    muted: "text-muted-foreground",
  } as const)[tone]);
</script>

<span
  class={cn("inline-flex items-center justify-center", toneClass, className)}
  role={label ? "status" : undefined}
  aria-label={label}
  aria-live={label ? "polite" : undefined}
  data-testid="loading-spinner"
>
  <svg
    class="animate-spinner-rotate motion-reduce:animate-none"
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    aria-hidden="true"
  >
    <circle
      cx="12"
      cy="12"
      r="9"
      stroke="currentColor"
      stroke-width={strokeWidth}
      stroke-linecap="round"
      stroke-dasharray="44"
      stroke-dashoffset="22"
      opacity="0.85"
    />
    <circle
      cx="12"
      cy="12"
      r="9"
      stroke="currentColor"
      stroke-width={strokeWidth}
      opacity="0.18"
    />
  </svg>
</span>
