<script lang="ts">
  /**
   * Single keycap chip, styled to match the canonical shortcuts catalog
   * (`/settings/shortcuts`). Fine typography lives in a scoped style block so
   * the palette can render sub-`text-xs` caps without design-token drift.
   */
  interface Props {
    label: string;
    large?: boolean;
  }

  let { label, large = false }: Props = $props();
</script>

<kbd class="keycap" class:keycap-lg={large}>{label}</kbd>

<style>
  .keycap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.125rem;
    height: 1.125rem;
    padding: 0 0.3125rem;
    border: 1px solid hsl(var(--border));
    border-bottom-width: 2px;
    border-radius: calc(var(--radius) - 0.25rem);
    background: hsl(var(--muted));
    color: hsl(var(--muted-foreground));
    font-family: ui-monospace, "SF Mono", SFMono-Regular, Menlo, monospace;
    font-size: var(--text-micro-lg);
    font-weight: 600;
    line-height: 1;
  }

  .keycap-lg {
    height: 1.25rem;
    min-width: 1.25rem;
    font-size: var(--text-caption);
  }
</style>
