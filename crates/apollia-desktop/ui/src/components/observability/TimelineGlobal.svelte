<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { GlobalTimelineEvent } from "$lib/types";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Select } from "$lib/components/ui/select";
  import {
    Clock,
    ChevronDown,
    ClipboardList,
    Wrench,
    Bot,
    Hand,
    Brain,
    Link2,
    AlertTriangle,
    type Icon,
  } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";

  const WINDOW_OPTIONS: { key: string; minutes: number; labelKey: string }[] = [
    { key: "30min", minutes: 30, labelKey: "observability.timeline_window_30min" },
    { key: "1h", minutes: 60, labelKey: "observability.timeline_window_1h" },
    { key: "6h", minutes: 360, labelKey: "observability.timeline_window_6h" },
    { key: "24h", minutes: 1440, labelKey: "observability.timeline_window_24h" },
    { key: "7d", minutes: 10080, labelKey: "observability.timeline_window_7d" },
  ];

  const TYPE_FILTERS = ["task", "tool", "llm", "hitl", "memory", "a2a", "error"] as const;
  type EventTypeFilter = (typeof TYPE_FILTERS)[number];

  const TYPE_LABEL_KEYS: Record<EventTypeFilter, string> = {
    task: "observability.type_task",
    tool: "observability.type_tool",
    llm: "observability.type_llm",
    hitl: "observability.type_hitl",
    memory: "observability.type_memory",
    a2a: "observability.type_a2a",
    error: "observability.type_error",
  };

  /** lucide-svelte icon components keyed by event type — visually consistent
   *  with the rest of the design system (no emojis). */
  const TYPE_ICON: Record<string, typeof Icon> = {
    task: ClipboardList,
    tool: Wrench,
    llm: Bot,
    hitl: Hand,
    memory: Brain,
    a2a: Link2,
    error: AlertTriangle,
  };

  /** Pastel chip palette per type. Foreground tokens are *dark* variants of
   *  each accent (info-foreground, warning-foreground…) so the text reads
   *  well on the 10% tinted background — never use `accent-foreground` here,
   *  it's white. LLM gets a custom dark violet distinct from primary/memory. */
  const TYPE_CHIP: Record<string, string> = {
    task: "bg-info/10 text-info border-info/30",
    tool: "bg-warning/10 text-warning border-warning/30",
    llm: "bg-[hsl(270_40%_55%/0.10)] text-[hsl(270_45%_38%)] border-[hsl(270_40%_55%/0.30)] dark:text-[hsl(270_55%_75%)]",
    hitl: "bg-destructive/10 text-destructive border-destructive/30",
    memory: "bg-primary/10 text-primary border-primary/30",
    a2a: "bg-success/10 text-success border-success/30",
    error: "bg-destructive/10 text-destructive border-destructive/30",
  };

  /** Color used for the bullet + icon on each event row (solid token color). */
  const TYPE_BULLET: Record<string, string> = {
    task: "hsl(var(--info))",
    tool: "hsl(var(--warning))",
    llm: "hsl(270 45% 50%)",
    hitl: "hsl(var(--destructive))",
    memory: "hsl(var(--primary))",
    a2a: "hsl(var(--success))",
    error: "hsl(var(--destructive))",
  };

  const REFRESH_INTERVAL_MS = 15_000;

  let windowMinutes = $state(60);
  let activeWindowKey = $derived(
    WINDOW_OPTIONS.find((w) => w.minutes === windowMinutes)?.key ?? "1h",
  );
  let enabledTypes = $state<Set<EventTypeFilter>>(new Set(TYPE_FILTERS));
  let agentFilter = $state<string>("all");
  let events = $state<GlobalTimelineEvent[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expandedKeys = $state<Set<string>>(new Set());
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  function agentOf(event: GlobalTimelineEvent): string | null {
    const m = /^\[([^\]]+)\]/.exec(event.summary);
    return m ? m[1] : null;
  }

  /** Strip the leading "[agent] " prefix for the visible event title — the agent
   *  is already shown on the right of the row. Keeps the title clean. */
  function eventBody(event: GlobalTimelineEvent): string {
    return event.summary.replace(/^\[[^\]]+\]\s*/, "");
  }

  let availableAgents = $derived(
    [...new Set(events.map(agentOf).filter((v): v is string => v !== null))].sort(),
  );

  let filteredEvents = $derived(
    events.filter((e) => {
      if (!enabledTypes.has(e.event_type as EventTypeFilter)) return false;
      if (agentFilter !== "all" && agentOf(e) !== agentFilter) return false;
      return true;
    }),
  );

  /** KPI counters — recompute against the currently filtered set. */
  let stats = $derived.by(() => {
    let tools = 0,
      llm = 0,
      errors = 0;
    for (const e of filteredEvents) {
      if (e.event_type === "tool") tools++;
      else if (e.event_type === "llm") llm++;
      else if (e.event_type === "error") errors++;
    }
    return { total: filteredEvents.length, tools, llm, errors };
  });

  /** Group events by day for sticky-style headers. */
  let groupedEvents = $derived.by(() => {
    const groups = new Map<string, GlobalTimelineEvent[]>();
    for (const e of filteredEvents) {
      const day = e.timestamp.slice(0, 10); // YYYY-MM-DD
      const list = groups.get(day) ?? [];
      list.push(e);
      groups.set(day, list);
    }
    return [...groups.entries()].sort((a, b) => b[0].localeCompare(a[0]));
  });

  function dayLabel(yyyyMmDd: string): string {
    const today = new Date().toISOString().slice(0, 10);
    const yesterday = new Date(Date.now() - 86_400_000).toISOString().slice(0, 10);
    if (yyyyMmDd === today) return $t("observability.timeline_group_today");
    if (yyyyMmDd === yesterday) return $t("observability.timeline_group_yesterday");
    const d = new Date(yyyyMmDd + "T12:00:00");
    return d.toLocaleDateString(undefined, {
      weekday: "long",
      day: "numeric",
      month: "long",
    });
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

  function toggleExpand(key: string) {
    const next = new Set(expandedKeys);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    expandedKeys = next;
  }

  function eventKey(e: GlobalTimelineEvent, index: number): string {
    return `${e.timestamp}::${index}`;
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

  function relativeTime(iso: string): string {
    if (!iso) return "";
    const then = new Date(iso).getTime();
    const now = Date.now();
    const diffSec = Math.max(0, Math.floor((now - then) / 1000));
    if (diffSec < 45) return $t("observability.timeline_relative_now");
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return $t("observability.timeline_relative_minutes", { values: { n: diffMin } });
    const diffH = Math.floor(diffMin / 60);
    if (diffH < 24) return $t("observability.timeline_relative_hours", { values: { n: diffH } });
    const diffD = Math.floor(diffH / 24);
    return $t("observability.timeline_relative_days", { values: { n: diffD } });
  }

  function handleWindowChange(minutes: number) {
    windowMinutes = minutes;
    loading = true;
    expandedKeys = new Set();
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

<div class="space-y-5">
  <!-- KPI strip — refreshes against current filter selection. -->
  {#if !loading}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-3" data-testid="timeline-kpis">
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
          {$t('observability.timeline_kpi_events')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">{stats.total}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
          {$t('observability.timeline_kpi_tools')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">{stats.tools}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
          {$t('observability.timeline_kpi_llm')}
        </div>
        <div class="text-[20px] font-semibold tabular-nums leading-none">{stats.llm}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta text-[10px] tracking-[1.4px] mb-1.5">
          {$t('observability.timeline_kpi_errors')}
        </div>
        <div
          class="text-[20px] font-semibold tabular-nums leading-none"
          class:text-destructive={stats.errors > 0}
        >
          {stats.errors}
        </div>
      </article>
    </div>
  {/if}

  <!-- Controls strip — window selector + type chips + agent picker. -->
  <Card class="px-4 py-3 flex flex-wrap items-center gap-x-5 gap-y-3" data-testid="timeline-controls">
    <div class="flex items-center gap-2">
      <span class="section-meta text-[10px] tracking-[1.4px]">
        {$t('observability.window')}
      </span>
      <div
        class="inline-flex items-center gap-0.5 rounded-md glass-border glass-surface p-0.5"
        role="tablist"
        aria-label={$t('observability.window')}
      >
        {#each WINDOW_OPTIONS as opt (opt.key)}
          {@const isActive = activeWindowKey === opt.key}
          <button
            type="button"
            role="tab"
            aria-selected={isActive}
            class="px-2.5 py-1 rounded text-[11.5px] font-medium tabular-nums transition-colors {isActive
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => handleWindowChange(opt.minutes)}
          >
            {$t(opt.labelKey)}
          </button>
        {/each}
      </div>
    </div>

    <div class="flex items-center gap-2 flex-wrap">
      <span class="section-meta text-[10px] tracking-[1.4px]">
        {$t('observability.filter')}
      </span>
      {#each TYPE_FILTERS as tf (tf)}
        {@const active = enabledTypes.has(tf)}
        {@const ChipIcon = TYPE_ICON[tf]}
        <button
          class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[11px] font-medium transition-colors {active
            ? TYPE_CHIP[tf]
            : 'glass-border-subtle glass-inset text-muted-foreground/50'}"
          onclick={() => toggleType(tf)}
        >
          <ChipIcon class="h-3 w-3" />
          <span>{$t(TYPE_LABEL_KEYS[tf])}</span>
        </button>
      {/each}
    </div>

    <div class="flex items-center gap-2 ml-auto">
      <label for="timeline-agent-filter" class="section-meta text-[10px] tracking-[1.4px]">
        {$t('observability.agent')}
      </label>
      <Select
        id="timeline-agent-filter"
        class="h-8 w-auto"
        bind:value={agentFilter}
        data-testid="timeline-agent-filter"
      >
        <option value="all">{$t('observability.all_agents_filter')}</option>
        {#each availableAgents as a (a)}
          <option value={a}>{a}</option>
        {/each}
      </Select>
    </div>
  </Card>

  <!-- Events list -->
  {#if loading}
    <div class="space-y-2">
      <Skeleton width="100%" height="3rem" />
      <Skeleton width="100%" height="3rem" />
      <Skeleton width="100%" height="3rem" />
    </div>
  {:else if error}
    <p class="text-sm text-destructive">{error}</p>
  {:else if filteredEvents.length === 0}
    <Card class="flex flex-col items-center justify-center py-16" data-testid="timeline-empty">
      <div class="rounded-full glass-inset p-4 mb-4">
        <Clock class="h-8 w-8 text-muted-foreground/60" />
      </div>
      <p class="text-[13px] text-muted-foreground">{$t('observability.empty_timeline')}</p>
    </Card>
  {:else}
    <div class="space-y-6">
      {#each groupedEvents as [day, dayEvents] (day)}
        <section>
          <header class="flex items-baseline gap-3 mb-2.5 px-1">
            <h4 class="m-0 text-[12.5px] font-semibold tracking-[-0.1px] text-foreground">
              {dayLabel(day)}
            </h4>
            <span class="section-meta text-[10px] tracking-[1.4px] /60">
              {dayEvents.length}
            </span>
          </header>

          <Card class="divide-y divide-border/30 overflow-hidden">
            {#each dayEvents as event, idx (eventKey(event, idx))}
              {@const key = eventKey(event, idx)}
              {@const isExpanded = expandedKeys.has(key)}
              {@const agent = agentOf(event)}
              {@const bulletColor = TYPE_BULLET[event.event_type] ?? "hsl(var(--muted))"}
              {@const RowIcon = TYPE_ICON[event.event_type] ?? ClipboardList}

              <div data-testid="timeline-event-{idx}">
                <button
                  type="button"
                  class="w-full flex items-start gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/40"
                  class:bg-muted={isExpanded}
                  onclick={() => toggleExpand(key)}
                  aria-expanded={isExpanded}
                >
                  <!-- Bullet + icon column -->
                  <div class="flex flex-col items-center pt-0.5 flex-shrink-0">
                    <span
                      class="inline-block h-2 w-2 rounded-full"
                      style="background-color: {bulletColor}"
                      aria-hidden="true"
                    ></span>
                    <RowIcon
                      class="mt-1.5 h-3.5 w-3.5"
                      style="color: {bulletColor};"
                      aria-hidden="true"
                    />
                  </div>

                  <!-- Title + chip + agent -->
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2 flex-wrap">
                      <span class="text-[13px] font-medium text-foreground">{eventBody(event)}</span>
                      <span
                        class="inline-flex items-center gap-1 rounded-full border px-2 py-[1px] text-[10.5px] font-medium {TYPE_CHIP[event.event_type] ?? 'glass-border-subtle glass-inset text-muted-foreground'}"
                      >
                        {$t(TYPE_LABEL_KEYS[event.event_type as EventTypeFilter] ?? "observability.type_task")}
                      </span>
                    </div>
                    <div class="mt-1 flex items-center gap-2 text-[11px] text-muted-foreground">
                      {#if agent}
                        <span class="truncate max-w-[14rem]">{agent}</span>
                        <span class="text-muted-foreground/40">·</span>
                      {/if}
                      <span class="tabular-nums">{formatTimestamp(event.timestamp)}</span>
                      <span class="text-muted-foreground/40">·</span>
                      <span class="tabular-nums">{relativeTime(event.timestamp)}</span>
                    </div>
                  </div>

                  <ChevronDown
                    class="h-3.5 w-3.5 text-muted-foreground/60 transition-transform flex-shrink-0 mt-1"
                    style={isExpanded ? "transform: rotate(180deg);" : ""}
                  />
                </button>

                {#if isExpanded}
                  <div class="px-4 pb-4 -mt-1">
                    <div class="rounded-lg glass-inset border border-border/30 p-3">
                      <pre class="overflow-x-auto text-[11.5px] font-mono leading-relaxed text-muted-foreground">{JSON.stringify(event.detail, null, 2)}</pre>
                    </div>
                  </div>
                {/if}
              </div>
            {/each}
          </Card>
        </section>
      {/each}
    </div>
  {/if}
</div>
