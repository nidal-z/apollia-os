<script lang="ts">
  /**
   * Barre d'occupation de la context window.
   *
   * Vert  <70 % — OK
   * Jaune 70-90 % — attention
   * Rouge >90 % — compression imminente
   */
  interface Props {
    used: number;
    max: number;
  }

  let { used, max }: Props = $props();

  const pct = $derived(max > 0 ? Math.min(100, (used / max) * 100) : 0);
  const tone = $derived(
    pct >= 90 ? "danger" : pct >= 70 ? "warn" : "ok",
  );
  const label = $derived(
    max > 0
      ? `${used.toLocaleString()} / ${max.toLocaleString()} tokens`
      : `${used.toLocaleString()} tokens`,
  );
</script>

<div class="flex flex-col gap-1" data-testid="context-window-bar" data-tone={tone}>
  <div class="flex items-baseline justify-between text-[11px]">
    <span class="text-muted-foreground">Context window</span>
    <span class="tabular-nums font-medium">{pct.toFixed(0)}%</span>
  </div>
  <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted/40">
    <div
      class="h-full transition-all duration-300"
      class:bg-success={tone === "ok"}
      class:bg-warning={tone === "warn"}
      class:bg-destructive={tone === "danger"}
      style="width: {pct}%"
    ></div>
  </div>
  <div class="text-[10px] text-muted-foreground/70 tabular-nums">{label}</div>
</div>

<style>
  .bg-success {
    background-color: rgb(34 197 94 / 0.9);
  }
  .bg-warning {
    background-color: rgb(234 179 8 / 0.9);
  }
  .bg-destructive {
    background-color: rgb(239 68 68 / 0.9);
  }
</style>
