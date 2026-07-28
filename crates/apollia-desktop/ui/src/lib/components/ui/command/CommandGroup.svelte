<script lang="ts">
  /**
   * Palette group header + rows. An optional `badge` renders a small pill next
   * to the label (used by the MRU "Recently used" group).
   */
  interface Props {
    label: string;
    badge?: string;
    children?: import("svelte").Snippet;
  }

  let { label, badge, children }: Props = $props();
</script>

<div role="group" aria-label={label} class="pb-1 pt-1">
  <div class="group-label">
    <span>{label}</span>
    {#if badge}
      <span class="badge">{badge}</span>
    {/if}
  </div>
  {@render children?.()}
</div>

<style>
  .group-label {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 0.4375rem;
    padding: 0.5rem 0.5rem 0.25rem;
    font-size: 0.65625rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: hsl(var(--muted-foreground));
  }

  .badge {
    font-size: 0.5625rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: none;
    padding: 0.0625rem 0.375rem;
    border-radius: 999px;
    background: hsl(var(--primary) / 0.12);
    color: hsl(var(--primary));
  }
</style>
