<script lang="ts">
  /**
   * PageTransition — fade + translate-y entrance wrapper (US-SP42-005).
   *
   * Drop this around the active route content so page changes get a polished
   * entrance. Honours `prefers-reduced-motion` automatically.
   */
  import type { Snippet } from "svelte";
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { prefersReducedMotion, duration } from "$lib/design/motion";

  interface Props {
    children: Snippet;
    /** Entrance duration in ms. Defaults to `--motion-base` (200ms). */
    duration?: number;
    /** Vertical offset in px. Positive = content slides up into place. */
    y?: number;
  }

  let {
    children,
    duration: dur = duration.base,
    y = 8,
  }: Props = $props();

  const reduced = prefersReducedMotion();
  const transitionDuration = $derived(reduced ? 0 : dur);
  const transitionY = $derived(reduced ? 0 : y);
</script>

<div
  class="h-full w-full"
  in:fly={{ y: transitionY, duration: transitionDuration, easing: cubicOut }}
  data-motion="page-transition"
>
  {@render children()}
</div>
