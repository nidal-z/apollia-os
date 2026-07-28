<script lang="ts">
  /**
   * Dimming overlay with a rounded cutout around the current tour anchor.
   *
   * Promoted from `operator/tours/`: the tour is no longer specific to the
   * operator mode. Two behaviours changed on the way.
   *
   * The rect is now a prop rather than something this component queries for
   * itself. The previous version measured on its own schedule while the runner
   * measured on another, so the hole and the step card could disagree.
   *
   * A missing anchor no longer produces a degenerate 1x1 cutout, which read as
   * an opaque black screen. The overlay simply does not render, and the card
   * falls back to its anchorless presentation.
   */
  interface Props {
    /** Anchor geometry, or `null` when the step has no resolvable anchor. */
    rect: DOMRect | null;
    /** Padding in pixels around the anchor. */
    padding?: number;
    /** Called when the user clicks the dimmed area outside the cutout. */
    onoverlayclick?: () => void;
  }

  let { rect, padding = 8, onoverlayclick }: Props = $props();

  /**
   * Counter-clockwise rounded rectangle.
   *
   * Combined with a clockwise outer rectangle under the nonzero fill rule, it
   * punches a transparent hole so the anchor stays visible and clickable.
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

  let clipPath = $derived.by(() => {
    if (rect === null) return "none";

    const viewportWidth = document.documentElement.clientWidth;
    const viewportHeight = document.documentElement.clientHeight;
    const outer = `M0,0 L${viewportWidth},0 L${viewportWidth},${viewportHeight} L0,${viewportHeight} Z`;

    const x = rect.left - padding;
    const y = rect.top - padding;
    const w = rect.width + padding * 2;
    const h = rect.height + padding * 2;
    const radius = Math.min(10, w / 2, h / 2);

    return `path("${outer} ${ccwRoundedRect(x, y, w, h, radius)}")`;
  });

  function handleOverlayClick(event: MouseEvent): void {
    event.stopPropagation();
    onoverlayclick?.();
  }
</script>

{#if rect !== null}
  <div
    class="spotlight-overlay"
    style:clip-path={clipPath}
    onclick={handleOverlayClick}
    role="presentation"
    aria-hidden="true"
    data-testid="tour-spotlight"
  ></div>
{/if}

<style>
  .spotlight-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay);
    background: var(--backdrop);
    cursor: default;
    transition: clip-path var(--motion-slow) var(--ease-standard);
  }

  @media (prefers-reduced-motion: reduce) {
    .spotlight-overlay {
      transition: none;
    }
  }
</style>
