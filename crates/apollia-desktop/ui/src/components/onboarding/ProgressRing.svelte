<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    /** Total number of segments in the ring. */
    segments: number;
    /** Number of completed (filled) segments. */
    completed: number;
    /** Outer diameter of the SVG in pixels. */
    size: number;
    /** Stroke width of the ring arc in pixels. */
    strokeWidth: number;
  }

  let { segments, completed, size, strokeWidth }: Props = $props();

  const radius = $derived((size - strokeWidth) / 2);
  const circumference = $derived(2 * Math.PI * radius);
  // Gap between segments as a fraction of the arc length of one segment.
  const gapFraction = 0.08;
  const segmentArc = $derived(circumference / segments);
  const segmentDash = $derived(segmentArc * (1 - gapFraction));
  const segmentGap = $derived(segmentArc * gapFraction);

  const pct = $derived(segments > 0 ? Math.round((completed / segments) * 100) : 0);
  const cx = $derived(size / 2);
  const cy = $derived(size / 2);

  // Rotate so the ring starts at the top (−90°).
  const rotate = "rotate(-90deg)";
  const transformOrigin = $derived(`${cx}px ${cy}px`);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 {size} {size}"
  aria-label={$t("a11y.progress_completed", { values: { pct } })}
  data-testid="progress-ring"
>
  <!-- Background track -->
  {#each Array(segments) as _, i}
    <circle
      cx={cx}
      cy={cy}
      r={radius}
      fill="none"
      stroke="rgba(52,53,245,0.10)"
      stroke-width={strokeWidth}
      stroke-dasharray="{segmentDash} {circumference - segmentDash}"
      stroke-dashoffset={-(segmentArc * i)}
      style="transform: {rotate}; transform-origin: {transformOrigin};"
    />
  {/each}

  <!-- Filled segments -->
  {#each Array(completed) as _, i}
    <circle
      cx={cx}
      cy={cy}
      r={radius}
      fill="none"
      stroke={i === completed - 1 ? "#3435f5" : "#7c5fd6"}
      stroke-width={strokeWidth}
      stroke-dasharray="{segmentDash} {circumference - segmentDash}"
      stroke-dashoffset={-(segmentArc * i)}
      style="
        transform: {rotate};
        transform-origin: {transformOrigin};
        transition: stroke-dashoffset 0.6s ease;
      "
    />
  {/each}

  <!-- Percentage label -->
  <text
    x={cx}
    y={cy}
    text-anchor="middle"
    dominant-baseline="central"
    font-size={size * 0.2}
    font-weight="700"
    fill="#1a1a2e"
  >{pct}%</text>
</svg>

<style>
  svg {
    display: block;
    flex-shrink: 0;
  }
</style>
