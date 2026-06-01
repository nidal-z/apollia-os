<script lang="ts">
  interface Props {
    /** Hide the cursor - used when the stream is interrupted or completed. */
    hidden?: boolean;
  }

  let { hidden = false }: Props = $props();
</script>

<span
  class="streaming-cursor"
  class:hidden
  aria-hidden="true"
  data-testid="streaming-cursor"
></span>

<style>
  .streaming-cursor {
    display: inline-block;
    width: 2px;
    height: 1.1em;
    margin-left: 1px;
    vertical-align: text-bottom;
    background-color: hsl(var(--primary));
    border-radius: 1px;
    animation: streaming-cursor-blink 1s steps(2, end) infinite;
  }

  .hidden {
    display: none;
  }

  @keyframes streaming-cursor-blink {
    0%,
    50% {
      opacity: 1;
    }
    50.01%,
    100% {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .streaming-cursor {
      animation: none;
      opacity: 0.8;
    }
  }
</style>
