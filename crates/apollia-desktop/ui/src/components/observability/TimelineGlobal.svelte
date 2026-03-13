<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { GlobalTimelineEvent } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";

  const WINDOW_OPTIONS: { label: string; minutes: number }[] = [
    { label: "30min", minutes: 30 },
    { label: "1h", minutes: 60 },
    { label: "6h", minutes: 360 },
    { label: "12h", minutes: 720 },
    { label: "24h", minutes: 1440 },
  ];

  const TYPE_FILTERS = ["task", "tool", "llm", "hitl"] as const;
  type EventTypeFilter = (typeof TYPE_FILTERS)[number];

  const TYPE_ICONS: Record<string, string> = {
    task: "\u{1F4CB}",
    tool: "\u{1F527}",
    llm: "\u{1F916}",
    hitl: "\u{270B}",
  };

  const TYPE_COLORS: Record<string, string> = {
    task: "bg-blue-500/10 text-blue-400 border-blue-500/30",
    tool: "bg-amber-500/10 text-amber-400 border-amber-500/30",
    llm: "bg-purple-500/10 text-purple-400 border-purple-500/30",
    hitl: "bg-red-500/10 text-red-400 border-red-500/30",
  };

  const REFRESH_INTERVAL_MS = 15_000;

  let windowMinutes = $state(60);
  let enabledTypes = $state<Set<EventTypeFilter>>(new Set(TYPE_FILTERS));
  let events = $state<GlobalTimelineEvent[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expandedIndices = $state<Set<number>>(new Set());
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  let filteredEvents = $derived(
    events.filter((e) => enabledTypes.has(e.event_type as EventTypeFilter)),
  );

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

  function toggleType(t: EventTypeFilter) {
    const next = new Set(enabledTypes);
    if (next.has(t)) {
      next.delete(t);
    } else {
      next.add(t);
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
      <span class="text-sm text-muted-foreground">Window:</span>
      <div class="flex gap-1 rounded-md border bg-muted/30 p-0.5">
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
      <span class="text-sm text-muted-foreground">Filter:</span>
      {#each TYPE_FILTERS as t (t)}
        <button
          class="flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs transition-colors {enabledTypes.has(t)
            ? TYPE_COLORS[t]
            : 'border-muted bg-muted/20 text-muted-foreground/50'}"
          onclick={() => toggleType(t)}
        >
          <span>{TYPE_ICONS[t]}</span>
          <span class="capitalize">{t}</span>
        </button>
      {/each}
    </div>
  </div>

  <!-- Events list -->
  {#if loading}
    <p class="text-sm text-muted-foreground">Loading timeline...</p>
  {:else if error}
    <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
  {:else if filteredEvents.length === 0}
    <!-- AC-5 — Empty state -->
    <div class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12">
      <p class="text-muted-foreground">Aucun événement dans cette période.</p>
    </div>
  {:else}
    <div class="space-y-1">
      {#each filteredEvents as event, index (event.timestamp + "-" + index)}
        <button
          class="flex w-full items-start gap-3 rounded-md border bg-card px-3 py-2 text-left transition-colors hover:bg-accent/30"
          onclick={() => toggleExpand(index)}
        >
          <!-- Type icon -->
          <span class="mt-0.5 text-sm">{TYPE_ICONS[event.event_type] ?? "\u{2022}"}</span>

          <!-- Content -->
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium">{event.summary}</span>
              <Badge variant="outline" class="text-[10px] capitalize">{event.event_type}</Badge>
            </div>
            <span class="text-xs text-muted-foreground">{formatTimestamp(event.timestamp)}</span>
          </div>

          <!-- Expand indicator -->
          <span class="mt-1 text-xs text-muted-foreground">
            {expandedIndices.has(index) ? "\u{25B2}" : "\u{25BC}"}
          </span>
        </button>

        {#if expandedIndices.has(index)}
          <div class="ml-8 rounded border bg-muted/20 p-3">
            <pre class="overflow-x-auto text-xs text-muted-foreground">{JSON.stringify(event.detail, null, 2)}</pre>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>
