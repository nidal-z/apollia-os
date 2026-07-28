<script lang="ts">
  /**
   * TranscriptWaveform - decorative amplitude bars for a transcript row.
   *
   * Purely visual: the desktop runtime does not persist the source audio, so
   * there is no playback. The bars encode nothing but a stable, per-row shape
   * (seeded from the id) that reads as "this came from audio" and anchors the
   * duration label. `aria-hidden`, so screen readers skip it entirely.
   *
   * `live` swaps the static bars for a staggered oscillation used by the
   * in-progress card. Both animation and the seeded shape collapse gracefully
   * under `prefers-reduced-motion`.
   */
  type Props = {
    /** Stable seed (transcript id) for deterministic bar heights. */
    seed?: string;
    /** Animate the bars for the transcribing-in-progress state. */
    live?: boolean;
    /** Number of bars. */
    count?: number;
  };

  let { seed = "", live = false, count = 48 }: Props = $props();

  // Deterministic LCG so a given row always renders the same silhouette.
  const bars = $derived.by(() => {
    let state = 0;
    for (let i = 0; i < seed.length; i += 1) {
      state = (state * 31 + seed.charCodeAt(i)) & 0x7fffffff;
    }
    if (state === 0) state = 7;
    const out: number[] = [];
    for (let i = 0; i < count; i += 1) {
      state = (state * 1103515245 + 12345) & 0x7fffffff;
      out.push(22 + (state % 1000) / 1000 * 78);
    }
    return out;
  });
</script>

<div
  class="flex h-7 min-w-0 flex-1 items-center gap-0.5 overflow-hidden"
  class:is-live={live}
  aria-hidden="true"
>
  {#each bars as height, i (i)}
    <span
      class="wave-bar w-0.5 shrink-0 rounded-full"
      class:bg-gradient-primary={live}
      class:bg-muted-foreground={!live}
      class:opacity-40={!live}
      style="height: {height}%; --bar-delay: {(i * 0.045).toFixed(3)}s;"
    ></span>
  {/each}
</div>

<style>
  .is-live .wave-bar {
    animation: wave-bar 1.1s ease-in-out infinite;
    animation-delay: var(--bar-delay);
    transform-origin: center;
  }
  @keyframes wave-bar {
    0%,
    100% {
      transform: scaleY(0.35);
    }
    50% {
      transform: scaleY(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .is-live .wave-bar {
      animation: none;
    }
  }
</style>
