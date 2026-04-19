<script lang="ts">
  /**
   * Urgency timer + progress bar for approval cards.
   *
   * Renders "Agent waiting since Xs" with a mm:ss counter and, when
   * `totalMs` is provided, an inline progress bar that switches colour once
   * the remaining budget falls below the warning / critical thresholds.
   *
   * The component owns its own `requestAnimationFrame` loop so multiple
   * mounted timers stay visually in sync with the user clock.
   */

  import { onMount, onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import { Clock } from "lucide-svelte";

  interface Props {
    /** Epoch-ms when the request was suspended. */
    startedAt: number;
    /** If set, enables a progress bar — timeout duration in ms. */
    totalMs?: number | null;
    /** Called when `totalMs` elapses. Optional. */
    onExpired?: () => void;
  }

  let { startedAt, totalMs = null, onExpired }: Props = $props();

  let now = $state<number>(Date.now());
  let rafId: number | undefined;
  let lastTick = 0;
  let expiredFired = false;

  function tick(ts: number): void {
    // Throttle to ~4 Hz — enough for mm:ss and smooth bar animation.
    if (ts - lastTick >= 250) {
      now = Date.now();
      lastTick = ts;
    }
    if (totalMs && !expiredFired && now - startedAt >= totalMs) {
      expiredFired = true;
      onExpired?.();
    }
    rafId = requestAnimationFrame(tick);
  }

  onMount(() => {
    rafId = requestAnimationFrame(tick);
  });

  onDestroy(() => {
    if (rafId !== undefined) cancelAnimationFrame(rafId);
  });

  const elapsedMs = $derived(Math.max(0, now - startedAt));
  const remainingMs = $derived(totalMs === null ? null : Math.max(0, totalMs - elapsedMs));
  const ratio = $derived(totalMs && totalMs > 0 ? Math.min(1, elapsedMs / totalMs) : 0);

  function format(ms: number): string {
    const total = Math.floor(ms / 1000);
    const m = Math.floor(total / 60);
    const s = total % 60;
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  }

  const urgency = $derived.by(() => {
    if (totalMs === null || remainingMs === null) return "normal";
    if (remainingMs <= 30_000) return "critical";
    if (remainingMs <= 120_000) return "warning";
    return "normal";
  });

  const barClass = $derived.by(() => {
    switch (urgency) {
      case "critical":
        return "bg-destructive";
      case "warning":
        return "bg-warning";
      default:
        return "bg-primary/60";
    }
  });

  const textClass = $derived.by(() => {
    switch (urgency) {
      case "critical":
        return "text-destructive";
      case "warning":
        return "text-warning";
      default:
        return "text-muted-foreground";
    }
  });
</script>

<div class="flex flex-col gap-0.5" data-testid="approval-timer" data-urgency={urgency}>
  <div class="flex items-center gap-1 text-[10px] {textClass}">
    <Clock class="h-3 w-3" aria-hidden="true" />
    <span>
      {$t("approvals.timer.waiting_since", { values: { elapsed: format(elapsedMs) } })}
    </span>
    {#if remainingMs !== null}
      <span aria-live="polite" class="ml-1 font-mono">
        {$t("approvals.timer.remaining", { values: { remaining: format(remainingMs) } })}
      </span>
    {/if}
  </div>
  {#if totalMs !== null}
    <div
      class="h-0.5 w-full overflow-hidden rounded-full bg-border/40"
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={Math.round(ratio * 100)}
    >
      <div
        class="h-full transition-[width] duration-200 ease-linear {barClass}"
        style:width="{ratio * 100}%"
      ></div>
    </div>
  {/if}
</div>
