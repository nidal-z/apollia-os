<script lang="ts" module>
  /** Local-calendar day key, YYYY-MM-DD in the machine's timezone. */
  export function localDayKey(date: Date): string {
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, "0");
    const d = String(date.getDate()).padStart(2, "0");
    return `${y}-${m}-${d}`;
  }

  /**
   * Day key of a wire timestamp. The wire carries UTC; grouping must follow
   * the machine's calendar, or an event at 01:00 local files under yesterday.
   * An unparsable input keeps its raw date prefix.
   */
  export function dayKeyOf(timestamp: string): string {
    const date = new Date(timestamp);
    return Number.isNaN(date.getTime()) ? timestamp.slice(0, 10) : localDayKey(date);
  }

  /** Classifies a day key against the local calendar for the group markers. */
  export function dayGroupKind(yyyyMmDd: string, now: Date): "today" | "yesterday" | "other" {
    if (yyyyMmDd === localDayKey(now)) return "today";
    const yesterday = new Date(now);
    yesterday.setDate(yesterday.getDate() - 1);
    if (yyyyMmDd === localDayKey(yesterday)) return "yesterday";
    return "other";
  }
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { fly } from "svelte/transition";
  import { t, locale } from "svelte-i18n";
  import type { GlobalTimelineEvent } from "$lib/types";
  import { getGlobalTimeline } from "$lib/ipc/timeline";
  import { rowIn } from "$lib/design/listMotion";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { ErrorBanner } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
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

  /** lucide-svelte icon components keyed by event type - visually consistent
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
   *  well on the 10% tinted background - never use `accent-foreground` here,
   *  it's white. LLM gets a custom dark violet distinct from primary/memory. */
  const TYPE_CHIP: Record<string, string> = {
    task: "bg-info/10 text-info border-info/30",
    tool: "bg-warning/10 text-warning border-warning/30",
    llm: "bg-[hsl(var(--chart-6)/0.10)] [color:hsl(var(--chart-6))] border-[hsl(var(--chart-6)/0.30)]",
    hitl: "bg-destructive/10 text-destructive border-destructive/30",
    memory: "bg-primary/10 text-primary border-primary/30",
    a2a: "bg-success/10 text-success border-success/30",
    error: "bg-destructive/10 text-destructive border-destructive/30",
  };

  /** Color used for the bullet + icon on each event row (solid token color). */
  const TYPE_BULLET: Record<string, string> = {
    task: "hsl(var(--info))",
    tool: "hsl(var(--warning))",
    llm: "hsl(var(--chart-6))",
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
  let errState = $state<HumanizedError | null>(null);
  let expandedKeys = $state<Set<string>>(new Set());
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  function agentOf(event: GlobalTimelineEvent): string | null {
    const m = /^\[([^\]]+)\]/.exec(event.summary);
    return m ? m[1] : null;
  }

  /** Strip the leading "[agent] " prefix for the visible event title - the agent
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

  /** KPI counters - recompute against the currently filtered set. */
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
      const day = dayKeyOf(e.timestamp);
      const list = groups.get(day) ?? [];
      list.push(e);
      groups.set(day, list);
    }
    return [...groups.entries()].sort((a, b) => b[0].localeCompare(a[0]));
  });

  function dayLabel(yyyyMmDd: string): string {
    const kind = dayGroupKind(yyyyMmDd, new Date());
    if (kind === "today") return $t("observability.timeline_group_today");
    if (kind === "yesterday") return $t("observability.timeline_group_yesterday");
    const d = new Date(yyyyMmDd + "T12:00:00");
    return d.toLocaleDateString($locale ?? "en", {
      weekday: "long",
      day: "numeric",
      month: "long",
    });
  }

  async function loadTimeline(): Promise<void> {
    try {
      events = await getGlobalTimeline(windowMinutes);
      errState = null;
    } catch (err: unknown) {
      errState = reportError(err, { surface: "inline" });
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
    return d.toLocaleTimeString($locale ?? "en", {
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
  <!-- KPI strip - refreshes against current filter selection. -->
  {#if !loading}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-3" data-testid="timeline-kpis">
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta mb-1.5">
          {$t('observability.timeline_kpi_events')}
        </div>
        <div class="text-heading-lg font-semibold tabular-nums leading-none">{stats.total}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta mb-1.5">
          {$t('observability.timeline_kpi_tools')}
        </div>
        <div class="text-heading-lg font-semibold tabular-nums leading-none">{stats.tools}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta mb-1.5">
          {$t('observability.timeline_kpi_llm')}
        </div>
        <div class="text-heading-lg font-semibold tabular-nums leading-none">{stats.llm}</div>
      </article>
      <article class="glass-inset rounded-lg px-4 py-3">
        <div class="section-meta mb-1.5">
          {$t('observability.timeline_kpi_errors')}
        </div>
        <div
          class="text-heading-lg font-semibold tabular-nums leading-none"
          class:text-destructive={stats.errors > 0}
        >
          {stats.errors}
        </div>
      </article>
    </div>
  {/if}

  <!-- Controls strip - window selector + type chips + agent picker. -->
  <Card class="px-4 py-3 flex flex-wrap items-center gap-x-5 gap-y-3" data-testid="timeline-controls">
    <div class="flex items-center gap-2">
      <span class="section-meta">
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
            class="px-2.5 py-1 rounded text-caption font-medium tabular-nums transition-colors {isActive
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
      <span class="section-meta">
        {$t('observability.filter')}
      </span>
      {#each TYPE_FILTERS as tf (tf)}
        {@const active = enabledTypes.has(tf)}
        {@const ChipIcon = TYPE_ICON[tf]}
        <button
          class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-caption font-medium transition-colors {active
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
      <label for="timeline-agent-filter" class="section-meta">
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
  {:else if errState}
    <ErrorBanner
      message={errState.friendly_message}
      onretry={() => { loading = true; void loadTimeline(); }}
      retryLabel={$t('common.retry')}
      data-testid="timeline-error"
    />
  {:else if filteredEvents.length === 0}
    <Card class="flex flex-col items-center justify-center py-16" data-testid="timeline-empty">
      <div class="rounded-full glass-inset p-4 mb-4">
        <Clock class="h-8 w-8 text-muted-foreground/60" />
      </div>
      <p class="text-body-sm text-muted-foreground">{$t('observability.empty_timeline')}</p>
    </Card>
  {:else}
    <div class="space-y-6">
      {#each groupedEvents as [day, dayEvents] (day)}
        <section>
          <header class="flex items-baseline gap-3 mb-2.5 px-1">
            <h4 class="m-0 text-body-sm font-semibold text-foreground">
              {dayLabel(day)}
            </h4>
            <span class="section-meta">
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

              <div data-testid="timeline-event-{idx}" in:fly={rowIn()}>
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
                      <span class="text-body-sm font-medium text-foreground">{eventBody(event)}</span>
                      <span
                        class="inline-flex items-center gap-1 rounded-full border px-2 py-[1px] text-caption font-medium {TYPE_CHIP[event.event_type] ?? 'glass-border-subtle glass-inset text-muted-foreground'}"
                      >
                        {$t(TYPE_LABEL_KEYS[event.event_type as EventTypeFilter] ?? "observability.type_task")}
                      </span>
                    </div>
                    <div class="mt-1 flex items-center gap-2 text-caption text-muted-foreground">
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
                      <pre class="overflow-x-auto text-code-sm font-mono leading-relaxed text-muted-foreground">{JSON.stringify(event.detail, null, 2)}</pre>
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
