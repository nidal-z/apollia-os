<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { TimelineEvent } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";

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

  const EVENT_ICONS: Record<string, string> = {
    task_transition: "\u{1F504}",
    step_started: "\u{25B6}\uFE0F",
    step_completed: "\u{2705}",
    llm_call: "\u{1F916}",
    tool_call: "\u{1F527}",
    hitl_suspended: "\u{23F8}\uFE0F",
    hitl_resolved: "\u{23EF}\uFE0F",
    task_completed: "\u{2714}\uFE0F",
  };

  async function fetchTimeline() {
    if (!taskId) return;
    loading = events.length === 0;
    error = null;
    try {
      const result: TimelineEvent[] = await invoke("get_task_timeline", {
        taskId,
      });
      events = result;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function toggleToolExpand(index: number) {
    const next = new Set(expandedTools);
    if (next.has(index)) {
      next.delete(index);
    } else {
      next.add(index);
    }
    expandedTools = next;
  }

  function formatRelativeTimestamp(iso: string): string {
    if (!iso) return "";
    const now = Date.now();
    const then = new Date(iso).getTime();
    const diffSecs = Math.floor((now - then) / 1000);
    if (diffSecs < 0) return "just now";
    if (diffSecs < 60) return `${diffSecs}s ago`;
    const diffMins = Math.floor(diffSecs / 60);
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    return new Date(iso).toLocaleString();
  }

  function formatDurationMs(ms: number | undefined): string {
    if (ms === undefined || ms === null) return "";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function formatCost(usd: number | undefined): string {
    if (usd === undefined || usd === null) return "";
    if (usd < 0.01) return `$${usd.toFixed(4)}`;
    return `$${usd.toFixed(2)}`;
  }

  function eventSummary(event: TimelineEvent): string {
    switch (event.type) {
      case "task_transition":
        return event.status;
      case "step_started":
        return event.tool ? `Step ${event.step_id} \u2014 ${event.tool}` : `Step ${event.step_id}`;
      case "step_completed":
        return `Step ${event.step_id} ${event.success ? "completed" : "failed"} ${formatDurationMs(event.duration_ms)}`;
      case "llm_call": {
        const tokens =
          event.prompt_tokens !== undefined && event.completion_tokens !== undefined
            ? `${event.prompt_tokens}+${event.completion_tokens} tokens`
            : "";
        const cost = formatCost(event.cost_usd);
        return [event.model, tokens, cost].filter(Boolean).join(" \u2014 ");
      }
      case "tool_call": {
        const dur = formatDurationMs(event.duration_ms);
        const exit = event.exit_code !== undefined && event.exit_code !== null ? `exit ${event.exit_code}` : "";
        return [event.tool_name, dur, exit].filter(Boolean).join(" \u2014 ");
      }
      case "hitl_suspended":
        return `Waiting for approval \u2014 ${event.prompt.slice(0, 80)}`;
      case "hitl_resolved": {
        const verdict = event.approved ? "Approved" : "Rejected";
        const wait = event.wait_ms !== undefined ? `wait ${formatDurationMs(event.wait_ms)}` : "";
        return [verdict, wait].filter(Boolean).join(" \u2014 ");
      }
      case "task_completed": {
        const dur = formatDurationMs(event.duration_ms);
        return dur ? `Completed in ${dur}` : "Completed";
      }
    }
  }

  $effect(() => {
    void fetchTimeline();

    let timer: ReturnType<typeof setInterval> | null = null;
    if (isRunning) {
      timer = setInterval(() => void fetchTimeline(), POLL_INTERVAL_MS);
    }

    return () => {
      if (timer !== null) {
        clearInterval(timer);
      }
    };
  });
</script>

<div class="space-y-1">
  {#if loading}
    <p class="text-sm text-muted-foreground">Loading timeline...</p>
  {:else if error}
    <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
  {:else if events.length === 0}
    <p class="text-sm text-muted-foreground">No timeline events yet.</p>
  {:else}
    <div class="relative ml-4 border-l border-border pl-6">
      {#each events as event, index (index)}
        <div class="relative mb-4">
          <!-- Dot on the timeline -->
          <span
            class="absolute -left-[calc(1.5rem+5px)] flex h-[10px] w-[10px] items-center justify-center rounded-full border bg-background text-[8px]"
          >
          </span>

          <div class="flex items-start gap-2">
            <span class="text-sm" title={event.type}>
              {EVENT_ICONS[event.type] ?? "\u{2022}"}
            </span>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium">{eventSummary(event)}</span>
                {#if event.type === "task_transition"}
                  <Badge variant="outline" class="text-[10px]">{event.status}</Badge>
                {/if}
              </div>
              <span class="text-xs text-muted-foreground">
                {formatRelativeTimestamp(event.timestamp)}
              </span>

              {#if event.type === "tool_call"}
                <button
                  class="mt-1 text-xs text-muted-foreground underline"
                  onclick={() => toggleToolExpand(index)}
                >
                  {expandedTools.has(index) ? "Hide details" : "Show details"}
                </button>
                {#if expandedTools.has(index)}
                  <div class="mt-1 rounded border bg-muted/30 p-2 text-xs">
                    <p>Tool: {event.tool_name}</p>
                    {#if event.duration_ms !== undefined}
                      <p>Duration: {formatDurationMs(event.duration_ms)}</p>
                    {/if}
                    {#if event.exit_code !== undefined && event.exit_code !== null}
                      <p>Exit code: {event.exit_code}</p>
                    {/if}
                    {#if event.truncated}
                      <p class="text-[var(--apollia-warning)]">[Output truncated]</p>
                    {/if}
                  </div>
                {/if}
              {/if}

              {#if event.type === "llm_call" && event.latency_ms !== undefined}
                <span class="text-xs text-muted-foreground">
                  Latency: {formatDurationMs(event.latency_ms)}
                </span>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
