<script lang="ts">
  import { AlertTriangle } from "lucide-svelte";
  import { tickStore } from "$lib/stores/inbox/timer";
  import { DEFAULT_URGENCY_THRESHOLD_MS } from "./types";

  interface Props {
    suspendedAt: string;
    thresholdMs?: number;
  }

  let { suspendedAt, thresholdMs = DEFAULT_URGENCY_THRESHOLD_MS }: Props = $props();

  // Derive elapsed from the shared tick — no per-item setInterval (E.31).
  const elapsed = $derived($tickStore - new Date(suspendedAt).getTime());
  const isUrgent = $derived(elapsed > thresholdMs);

  function format(ms: number): string {
    const total = Math.max(0, Math.floor(ms / 1000));
    const mins = Math.floor(total / 60);
    const secs = total % 60;
    if (mins >= 60) {
      const hours = Math.floor(mins / 60);
      return `${hours}h ${mins % 60}m`;
    }
    if (mins > 0) return `${mins}m ${secs}s`;
    return `${secs}s`;
  }
</script>

<span
  class="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[10px] font-medium {isUrgent
    ? 'bg-destructive/10 text-destructive ring-1 ring-destructive/40 animate-pulse'
    : 'bg-muted text-muted-foreground'}"
  data-testid="inbox-urgency-badge"
  data-urgent={isUrgent}
  aria-label={isUrgent ? "Urgent" : "Elapsed time"}
>
  {#if isUrgent}
    <AlertTriangle size={10} strokeWidth={2} />
  {/if}
  {format(elapsed)}
</span>
