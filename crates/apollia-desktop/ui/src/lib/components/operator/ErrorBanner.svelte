<script lang="ts">
  /**
   * ErrorBanner - canonical persistent error/notice banner.
   *
   * Full-width banner for persistent issues (failed load, runtime down, config
   * error), rendered at the top of a content area. Transient action errors
   * (a failed save, a copy error, ...) should use a toast instead.
   */
  import type { Snippet } from "svelte";
  import { AlertTriangle, Info, XCircle, RefreshCw, X } from "lucide-svelte";

  type Tone = "danger" | "warning" | "info";

  interface Props {
    message?: string;
    tone?: Tone;
    /** Optional retry handler; renders a Retry button. */
    onretry?: () => void;
    /** Label for the retry button (defaults to "Retry"). */
    retryLabel?: string;
    /** Whether a retry is in progress (spins the icon, disables the button). */
    loading?: boolean;
    /** Optional dismiss handler; renders a close button. */
    ondismiss?: () => void;
    /** Custom content; overrides `message`. */
    children?: Snippet;
    class?: string;
    "data-testid"?: string;
  }

  let {
    message,
    tone = "danger",
    onretry,
    retryLabel = "Retry",
    loading = false,
    ondismiss,
    children,
    class: className = "",
    "data-testid": testid,
  }: Props = $props();

  const toneClass = $derived(
    tone === "danger"
      ? "border-destructive/30 bg-destructive/5 text-danger-a11y"
      : tone === "warning"
        ? "border-warning/30 bg-warning/5 text-warning-a11y"
        : "border-info/30 bg-info/5 text-info",
  );

  const Icon = $derived(tone === "danger" ? XCircle : tone === "warning" ? AlertTriangle : Info);
</script>

<div
  role="alert"
  data-testid={testid}
  class="flex items-center gap-2.5 rounded-lg border px-3.5 py-2.5 text-[12.5px] {toneClass} {className}"
>
  <Icon size={14} class="shrink-0" />
  <div class="min-w-0 flex-1">
    {#if children}{@render children()}{:else}{message}{/if}
  </div>
  {#if onretry}
    <button
      type="button"
      onclick={onretry}
      disabled={loading}
      class="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-current/30 px-2.5 py-1 text-[11.5px] font-medium transition-colors hover:bg-current/10 disabled:opacity-50"
    >
      <RefreshCw size={11} class={loading ? "animate-spin" : ""} />
      {retryLabel}
    </button>
  {/if}
  {#if ondismiss}
    <button
      type="button"
      onclick={ondismiss}
      aria-label="Dismiss"
      class="shrink-0 opacity-60 transition-opacity hover:opacity-100"
    >
      <X size={13} />
    </button>
  {/if}
</div>
