<script lang="ts">
  /**
   * One-shot confetti burst for the onboarding completion payoff.
   *
   * Purely decorative (aria-hidden), token-coloured, and self-contained. It
   * renders nothing under reduced-motion so the celebration stays calm for
   * users who opt out of animation.
   */
  import { prefersReducedMotion } from "$lib/design/motion";

  const COLORS = [
    "var(--grad-a)",
    "var(--grad-b)",
    "var(--success)",
    "var(--warning)",
  ] as const;

  interface Piece {
    left: number;
    delay: number;
    rotate: number;
    color: string;
  }

  const pieces: Piece[] = prefersReducedMotion()
    ? []
    : Array.from({ length: 26 }, (_, i) => ({
        left: Math.random() * 100,
        delay: Math.random() * 0.4,
        rotate: Math.random() * 360,
        color: COLORS[i % COLORS.length],
      }));
</script>

{#if pieces.length > 0}
  <div class="confetti" aria-hidden="true">
    {#each pieces as piece}
      <i
        style="left: {piece.left}%; background: hsl({piece.color}); animation-delay: {piece.delay}s; transform: rotate({piece.rotate}deg);"
      ></i>
    {/each}
  </div>
{/if}

<style>
  .confetti {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: hidden;
  }

  .confetti i {
    position: absolute;
    top: -0.75rem;
    width: 0.5rem;
    height: 0.75rem;
    border-radius: 0.125rem;
    opacity: 0;
    animation: confetti-fall 1.4s var(--ease-standard) forwards;
  }

  @keyframes confetti-fall {
    0% {
      opacity: 0;
      transform: translateY(-0.625rem) rotate(0);
    }
    12% {
      opacity: 1;
    }
    100% {
      opacity: 0;
      transform: translateY(22.5rem) rotate(540deg);
    }
  }
</style>
