<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { GlobalTimelineEvent } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Clock } from "lucide-svelte";

  interface Props {
    /** Drill-down callback — fires with the correlation id (step_id / tool_id / hitl_id). */
    onDrillDown?: (correlationId: string) => void;
  }

  let { onDrillDown }: Props = $props();

  const WINDOW_OPTIONS: { label: string; minutes: number }[] = [
    { label: "30min", minutes: 30 },
    { label: "1h", minutes: 60 },
    { label: "6h", minutes: 360 },
    { label: "12h", minutes: 720 },
    { label: "24h", minutes: 1440 },
  ];

  const TYPE_FILTERS = ["task", "tool", "llm", "hitl", "memory", "a2a", "error"] as const;
  type EventTypeFilter = (typeof TYPE_FILTERS)[number];

  const TYPE_ICONS: Record<string, string> = {
    task: "\u{1F4CB}",
    tool: "\u{1F527}",
    llm: "\u{1F916}",
    hitl: "\u{270B}",
    memory: "\u{1F9E0}",
    a2a: "\u{1F517}",
    error: "\u{26A0}",
  };

  const TYPE_COLORS: Record<string, string> = {
    task: "bg-info/10 text-info border-info/30",
    tool: "bg-warning/10 text-warning border-warning/30",
    llm: "bg-accent/10 text-accent-foreground border-accent/30",
    hitl: "bg-destructive/10 text-destructive border-destructive/30",
    memory: "bg-primary/10 text-primary border-primary/30",
    a2a: "bg-success/10 text-success border-success/30",
    error: "bg-destructive/10 text-destructive border-destructive/30",
  };

  const SCRUBBER_MARKER_COLORS: Record<string, string> = {
    task: "hsl(var(--info))",
    tool: "hsl(var(--warning))",
    llm: "hsl(var(--accent-foreground))",
    hitl: "hsl(var(--destructive))",
    memory: "hsl(var(--primary))",
    a2a: "hsl(var(--success))",
    error: "hsl(var(--destructive))",
  };

  const REFRESH_INTERVAL_MS = 15_000;

  let windowMinutes = $state(60);
  let enabledTypes = $state<Set<EventTypeFilter>>(new Set(TYPE_FILTERS));
  let events = $state<GlobalTimelineEvent[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expandedIndices = $state<Set<number>>(new Set());
  let scrubberCursor = $state(0);
  let hoverIndex = $state<number | null>(null);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  let filteredEvents = $derived(
    events.filter((e) => enabledTypes.has(e.event_type as EventTypeFilter)),
  );

  // Extract correlation id from event.detail for drill-down.
  function correlationIdFor(event: GlobalTimelineEvent): string | null {
    const detail = event.detail as Record<string, unknown>;
    const candidates = ["step_id", "tool_call_id", "hitl_id", "correlation_id"];
    for (const key of candidates) {
      const raw = detail?.[key];
      if (typeof raw === "string" && raw.length > 0) return raw;
    }
    return null;
  }

  async function loadTimeline(): Promise<void> {
    try {
      const result: GlobalTimelineEvent[] = await invoke("get_global_timeline", {
        params: { window_minutes: windowMinutes },
      });
      events = result;
      error = null;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function toggleType(tf: EventTypeFilter) {
    const next = new Set(enabledTypes);
    if (next.has(tf)) {
      next.delete(tf);
    } else {
      next.add(tf);
    }
    enabledTypes = next;
  }

  function toggleExpand(index: number) {
    const next = new Set(expandedIndices);
    if (next.has(index)) {
      next.delete(index);
    } else {
      next.add(index);
    }
    expandedIndices = next;
  }

  function handleMarkerClick(event: GlobalTimelineEvent, index: number) {
    scrubberCursor = index;
    const corr = correlationIdFor(event);
    if (corr) onDrillDown?.(corr);
    else toggleExpand(index);
  }

  function formatTimestamp(iso: string): string {
    if (!iso) return "";
    const d = new Date(iso);
    return d.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  function handleWindowChange(minutes: number) {
    windowMinutes = minutes;
    loading = true;
    expandedIndices = new Set();
    void loadTimeline();
  }

  onMount(() => {
    void loadTimeline();
    refreshTimer = setInterval(() => {
      void loadTimeline();
    }, REFRESH_INTERVAL_MS);
  });

  onDestroy(() => {
    if (refreshTimer !== null) {
      clearInterval(refreshTimer);
      refreshTimer = null;
    }
  });
</script>

<div class="space-y-4">
  <!-- Controls: Window slider + Type filters -->
  <div class="flex flex-wrap items-center gap-4">
    <!-- Window selector -->
    <div class="flex items-center gap-2">
      <span class="text-sm text-muted-foreground">{$t('observability.window')}:</span>
      <div class="flex gap-1 rounded-md glass-border glass-surface p-0.5">
        {#each WINDOW_OPTIONS as opt (opt.minutes)}
          <button
            class="rounded px-2 py-0.5 text-xs font-medium transition-colors {windowMinutes === opt.minutes
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => handleWindowChange(opt.minutes)}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    </div>

    <!-- Type filters -->
    <div class="flex items-center gap-2">
      <span class="text-sm text-muted-foreground">{$t('observability.filter')}:</span>
      {#each TYPE_FILTERS as tf (tf)}
        <button
          class="flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs transition-colors {enabledTypes.has(tf)
            ? TYPE_COLORS[tf]
            : 'glass-border-subtle glass-inset text-muted-foreground/50'}"
          onclick={() => toggleType(tf)}
        >
          <span>{TYPE_ICONS[tf]}</span>
          <span class="capitalize">{tf}</span>
        </button>
      {/each}
    </div>
  </div>

  <!-- Horizontal scrubber with event markers -->
  {#if !loading && !error && filteredEvents.length > 0}
    <div class="scrubber" data-testid="timeline-scrubber">
      <div class="scrubber-track">
        {#each filteredEvents as event, index (event.timestamp + "-" + index)}
          <button
            class="scrubber-marker"
            class:active={scrubberCursor === index}
            style:left="{(index / Math.max(1, filteredEvents.length - 1)) * 100}%"
            style:background-color={SCRUBBER_MARKER_COLORS[event.event_type] ?? "hsl(var(--muted))"}
            data-testid="scrubber-marker-{index}"
            aria-label={event.summary}
            onclick={() => handleMarkerClick(event, index)}
            onmouseenter={() => (hoverIndex = index)}
            onmouseleave={() => (hoverIndex = null)}
          ></button>
        {/each}
      </div>

      {#if hoverIndex !== null && filteredEvents[hoverIndex]}
        <div
          class="scrubber-preview"
          data-testid="scrubber-preview"
          style:left="{(hoverIndex / Math.max(1, filteredEvents.length - 1)) * 100}%"
        >
          <span class="preview-time">
            {formatTimestamp(filteredEvents[hoverIndex].timestamp)}
          </span>
          <span class="preview-summary">{filteredEvents[hoverIndex].summary}</span>
        </div>
      {/if}
    </div>
  {/if}

  <!-- Events list -->
  {#if loading}
    <div class="space-y-2">
      <Skeleton width="100%" height="2.5rem" />
      <Skeleton width="100%" height="2.5rem" />
      <Skeleton width="100%" height="2.5rem" />
    </div>
  {:else if error}
    <p class="text-sm text-destructive">{error}</p>
  {:else if filteredEvents.length === 0}
    <!-- Empty state -->
    <div class="glass-card glass-border flex flex-col items-center justify-center py-12" data-testid="timeline-empty">
      <Clock class="h-12 w-12 text-muted-foreground mb-4 opacity-50" />
      <p class="text-muted-foreground">{$t('observability.empty_timeline')}</p>
    </div>
  {:else}
    <div class="space-y-1.5">
      {#each filteredEvents as event, index (event.timestamp + "-" + index)}
        <!-- glass-card-hover event cards -->
        <button
          class="flex w-full items-start gap-3 glass-card-hover px-3.5 py-2.5 text-left"
          class:ring-primary={scrubberCursor === index}
          onclick={() => handleMarkerClick(event, index)}
          data-testid="timeline-event-{index}"
        >
          <span class="mt-0.5 text-sm">{TYPE_ICONS[event.event_type] ?? "\u{2022}"}</span>

          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="text-[13px] font-medium">{event.summary}</span>
              <Badge variant="secondary" class="text-[10px] capitalize">{event.event_type}</Badge>
              {#if correlationIdFor(event)}
                <Badge variant="outline" class="text-[10px] font-mono">
                  {correlationIdFor(event)}
                </Badge>
              {/if}
            </div>
            <span class="text-[11px] text-muted-foreground">{formatTimestamp(event.timestamp)}</span>
          </div>

          <span class="mt-1 text-[11px] text-muted-foreground">
            {expandedIndices.has(index) ? "\u{25B2}" : "\u{25BC}"}
          </span>
        </button>

        {#if expandedIndices.has(index)}
          <div class="ml-8 rounded glass-border glass-inset p-3">
            <pre class="overflow-x-auto text-xs text-muted-foreground">{JSON.stringify(event.detail, null, 2)}</pre>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .scrubber {
    position: relative;
    height: 2.5rem;
    padding: 0.75rem 0;
  }
  .scrubber-track {
    position: relative;
    width: 100%;
    height: 3px;
    border-radius: 9999px;
    background-color: hsl(var(--border));
  }
  .scrubber-marker {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 2px solid hsl(var(--background));
    padding: 0;
    cursor: pointer;
    transition: transform 120ms ease;
  }
  .scrubber-marker:hover,
  .scrubber-marker.active {
    transform: translate(-50%, -50%) scale(1.4);
    box-shadow: 0 0 0 2px hsl(var(--primary) / 0.25);
  }
  .scrubber-preview {
    position: absolute;
    bottom: 100%;
    transform: translateX(-50%);
    padding: 0.25rem 0.5rem;
    border-radius: 0.375rem;
    background-color: hsl(var(--popover));
    border: 1px solid hsl(var(--border));
    font-size: 11px;
    white-space: nowrap;
    pointer-events: none;
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    margin-bottom: 0.25rem;
    box-shadow: 0 4px 12px hsl(var(--foreground) / 0.08);
  }
  .preview-time {
    color: hsl(var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }
  .preview-summary {
    font-weight: 500;
  }
</style>
