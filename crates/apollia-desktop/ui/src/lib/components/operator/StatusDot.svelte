<script lang="ts">
  interface Props {
    /** CSS color string (e.g. "hsl(var(--primary))"). Defaults to primary. */
    color?: string;
    glow?: boolean;
    size?: number;
  }

  let { color = "hsl(var(--primary))", glow = false, size = 6 }: Props = $props();
</script>

<span
  class="inline-block rounded-full {glow ? 'animate-glow-pulse' : ''}"
  style="width: {size}px; height: {size}px; background: {color}; box-shadow: {glow
    ? `0 0 6px ${color}`
    : 'none'};"
></span>
