<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    /** CSS selector of the element to highlight. */
    targetSelector: string;
    /** Padding in pixels around the target element. */
    padding?: number;
    /** Clip-path transition duration in milliseconds. */
    animationDuration?: number;
    /** Whether the overlay is visible. */
    visible?: boolean;
    /** Called when the user clicks on the opaque overlay region (outside the cutout). */
    onoverlaclick?: () => void;
  }

  let {
    targetSelector,
    padding = 8,
    animationDuration = 400,
    visible = true,
    onoverlaclick,
  }: Props = $props();

  let clipPath = $state('none');

  /**
   * Builds a counter-clockwise rounded rectangle SVG path fragment.
   *
   * Combined with a clockwise outer rectangle and the nonzero fill rule, the inner
   * CCW path produces a transparent hole in the overlay.
   */
  function ccwRoundedRect(x: number, y: number, w: number, h: number, r: number): string {
    return (
      `M${x + r},${y}` +
      ` Q${x},${y} ${x},${y + r}` +
      ` L${x},${y + h - r}` +
      ` Q${x},${y + h} ${x + r},${y + h}` +
      ` L${x + w - r},${y + h}` +
      ` Q${x + w},${y + h} ${x + w},${y + h - r}` +
      ` L${x + w},${y + r}` +
      ` Q${x + w},${y} ${x + w - r},${y}` +
      ` Z`
    );
  }

  /**
   * Computes the clip-path value for the overlay.
   *
   * Uses a clockwise outer rectangle + a counter-clockwise inner rounded rectangle.
   * With the CSS nonzero fill rule, the inner rect creates a transparent hole so the
   * target element remains visible and interactive through the overlay.
   *
   * When rect is null, a degenerate 1×1 inner rect preserves the path structure for
   * smooth CSS transitions without producing a visible hole.
   */
  function buildClipPath(rect: DOMRect | null): string {
    const W = document.documentElement.clientWidth;
    const H = document.documentElement.clientHeight;

    // CW outer rectangle covers the entire viewport.
    const outer = `M0,0 L${W},0 L${W},${H} L0,${H} Z`;

    if (rect === null) {
      const cx = W / 2;
      const cy = H / 2;
      return `path("${outer} ${ccwRoundedRect(cx - 0.5, cy - 0.5, 1, 1, 0)}")`;
    }

    const x = rect.left - padding;
    const y = rect.top - padding;
    const w = rect.width + padding * 2;
    const h = rect.height + padding * 2;
    const r = Math.min(8, w / 2, h / 2);

    return `path("${outer} ${ccwRoundedRect(x, y, w, h, r)}")`;
  }

  function refresh(): void {
    if (!targetSelector) {
      clipPath = buildClipPath(null);
      return;
    }

    const el = document.querySelector(targetSelector);

    if (el === null) {
      console.warn(`[TourSpotlight] selector not found: "${targetSelector}"`);
      clipPath = buildClipPath(null);
      return;
    }

    clipPath = buildClipPath(el.getBoundingClientRect());
  }

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleRefresh(): void {
    if (debounceTimer !== null) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(refresh, 100);
  }

  // ─── Layout-settle loop ──────────────────────────────────────────────────
  // After the selector changes (new step / navigation) the target element may
  // shift as CSS transitions and lazy content settle.  We poll via rAF for up
  // to 1.5 s so the clip-path tracks the element in real time.

  let settleRaf: number | null = null;

  function stopSettle(): void {
    if (settleRaf !== null) {
      cancelAnimationFrame(settleRaf);
      settleRaf = null;
    }
  }

  function startSettle(): void {
    stopSettle();
    const startTime = performance.now();
    const SETTLE_MS = 1500;

    function poll(): void {
      refresh();
      if (performance.now() - startTime < SETTLE_MS) {
        settleRaf = requestAnimationFrame(poll);
      } else {
        settleRaf = null;
      }
    }

    settleRaf = requestAnimationFrame(poll);
  }

  // Re-compute clip-path whenever targetSelector or padding changes.
  $effect(() => {
    // Access reactive deps so Svelte tracks them.
    void targetSelector;
    void padding;
    refresh();
    startSettle();
  });

  onMount(() => {
    window.addEventListener('resize', scheduleRefresh);
    window.addEventListener('scroll', scheduleRefresh, { passive: true });

    return () => {
      window.removeEventListener('resize', scheduleRefresh);
      window.removeEventListener('scroll', scheduleRefresh);
      stopSettle();
      if (debounceTimer !== null) clearTimeout(debounceTimer);
    };
  });

  function handleOverlayClick(e: MouseEvent): void {
    e.stopPropagation();
    onoverlaclick?.();
  }
</script>

{#if visible}
  <div
    class="spotlight-overlay"
    style:clip-path={clipPath}
    style:transition={`clip-path ${animationDuration}ms var(--ease-standard)`}
    onclick={handleOverlayClick}
    role="presentation"
    data-testid="tour-spotlight"
  ></div>
{/if}

<style>
  .spotlight-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgba(0, 0, 0, 0.6);
    cursor: default;
  }
</style>
