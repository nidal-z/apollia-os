<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import type { TimelineEvent } from "$lib/types";
  import { formatRelativeTime } from "$lib/utils";
  import { Badge } from "$lib/components/ui/badge";
  import { RefreshCw, ChevronDown, ChevronRight } from "lucide-svelte";

  interface Props {
    taskId: string;
    isRunning: boolean;
  }

  let { taskId, isRunning }: Props = $props();

  let events = $state<TimelineEvent[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let expandedTools = $state<Set<number>>(new Set());

  const POLL_INTERVAL_MS = 2000;

  const STATUS_I18N: Record<string, string> = {
    working: "dashboard.status_working",
    submitted: "dashboard.status_submitted",
    completed: "dashboard.status_completed",
    failed: "dashboard.status_failed",
    input_required: "dashboard.status_approval",
    canceled: "dashboard.status_canceled",
  };

  const EVENT_DOT_COLOR: Record<string, string> = {
    task_transition: "bg-primary",
    step_started: "bg-info",
    step_completed: "bg-success",
    llm_call: "bg-secondary",
    tool_call: "bg-muted-foreground",
    hitl_suspended: "bg-warning",
    hitl_resolved: "bg-success",
    task_completed: "bg-success",
  };

  async function fetchTimeline() {
    if (!taskId) return;
    loading = events.length === 0;
    error = null;
    try {
      const result: TimelineEvent[] = await invoke("get_task_timeline", { taskId });
      events = result;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function toggleToolExpand(index: number) {
    const next = new Set(expandedTools);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    expandedTools = next;
  }

  function formatDurationMs(ms: number | undefined): string {
    if (ms === undefined || ms === null) return "";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function formatCost(usd: number | undefined): string {
    if (usd === undefined || usd === null) return "";
    return usd < 0.01 ? `$${usd.toFixed(4)}` : `$${usd.toFixed(2)}`;
  }

  function eventSummary(event: TimelineEvent): string {
    switch (event.type) {
      case "task_transition": return $t(STATUS_I18N[event.status] ?? "dashboard.status_submitted");
      case "step_started": return event.tool ? `Step ${event.step_id} — ${event.tool}` : `Step ${event.step_id}`;
      case "step_completed": return `Step ${event.step_id} ${event.success ? "done" : "failed"} ${formatDurationMs(event.duration_ms)}`;
      case "llm_call": {
        const tokens = event.prompt_tokens !== undefined && event.completion_tokens !== undefined
          ? `${event.prompt_tokens}+${event.completion_tokens}t` : "";
        return [event.model, tokens, formatCost(event.cost_usd)].filter(Boolean).join(" · ");
      }
      case "tool_call": return [event.tool_name, formatDurationMs(event.duration_ms), event.exit_code !== undefined && event.exit_code !== null ? `exit ${event.exit_code}` : ""].filter(Boolean).join(" · ");
      case "hitl_suspended": return `Waiting — ${event.prompt.slice(0, 60)}`;
      case "hitl_resolved": return event.approved ? $t('common.approved') : $t('common.rejected');
      case "task_completed": return formatDurationMs(event.duration_ms) ? `Done in ${formatDurationMs(event.duration_ms)}` : "Done";
    }
  }

  $effect(() => {
    void fetchTimeline();
    let timer: ReturnType<typeof setInterval> | null = null;
    if (isRunning) timer = setInterval(() => void fetchTimeline(), POLL_INTERVAL_MS);
    return () => { if (timer !== null) clearInterval(timer); };
  });
</script>

<div class="space-y-0.5">
  {#if loading}
    <div class="flex items-center gap-2 py-3">
      <RefreshCw size={12} class="animate-spin text-muted-foreground" />
      <span class="text-[11px] text-muted-foreground">{$t('tasks.loading_timeline')}</span>
    </div>
  {:else if error}
    <p class="text-[11px] text-destructive py-2">{error}</p>
  {:else if events.length === 0}
    <p class="text-[11px] text-muted-foreground py-2">{$t('tasks.no_timeline')}</p>
  {:else}
    <div class="relative ml-2 border-l border-border/50 pl-4 space-y-2" data-testid="task-timeline">
      {#each events as event, index (index)}
        <div
          class="relative"
          data-testid="timeline-event"
          data-event-type={event.type}
          in:fly={{ y: 3, duration: 100, delay: index * 15 }}
        >
          <!-- Dot -->
          <span class="absolute -left-[calc(1rem+3px)] top-1.5 h-1.5 w-1.5 rounded-full {EVENT_DOT_COLOR[event.type] ?? 'bg-muted-foreground'}"></span>

          <div class="flex items-baseline gap-2">
            <span class="text-[11px] text-foreground/70">{eventSummary(event)}</span>
            {#if event.type === "task_transition"}
              <Badge variant="outline" class="text-[8px] px-1 py-0">{$t(STATUS_I18N[event.status] ?? "dashboard.status_submitted")}</Badge>
            {/if}
            <span class="ml-auto shrink-0 text-[10px] text-muted-foreground/35">{formatRelativeTime(event.timestamp)}</span>
          </div>

          <!-- Tool call expandable -->
          {#if event.type === "tool_call"}
            <button
              class="mt-0.5 flex items-center gap-1 text-[10px] text-muted-foreground/50 hover:text-foreground transition-colors"
              onclick={() => toggleToolExpand(index)}
            >
              {#if expandedTools.has(index)}
                <ChevronDown size={10} />
              {:else}
                <ChevronRight size={10} />
              {/if}
              {$t('tasks.show_details')}
            </button>
            {#if expandedTools.has(index)}
              <div class="mt-1 rounded-md bg-muted/40 px-2.5 py-1.5 text-[10px] text-muted-foreground space-y-0.5">
                <p>{$t('tasks.tool_label')}: {event.tool_name}</p>
                {#if event.duration_ms !== undefined}<p>{$t('tasks.duration_label')}: {formatDurationMs(event.duration_ms)}</p>{/if}
                {#if event.exit_code !== undefined && event.exit_code !== null}<p>{$t('tasks.exit_code_label')}: {event.exit_code}</p>{/if}
                {#if event.truncated}<p class="text-warning">{$t('tasks.output_truncated')}</p>{/if}
              </div>
            {/if}
          {/if}

          {#if event.type === "llm_call" && event.latency_ms !== undefined}
            <span class="text-[10px] text-muted-foreground/35">{$t('tasks.latency')}: {formatDurationMs(event.latency_ms)}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
