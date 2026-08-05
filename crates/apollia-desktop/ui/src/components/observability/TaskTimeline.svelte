<script lang="ts" module>
  import type { TaskTimelineEvent } from "$lib/ipc/tasks";

  /** Visual family of a row: drives the icon and the accent token. */
  export type TimelineRowTone = "task" | "step" | "tool" | "llm" | "hitl";

  /** One machine fact rendered as a `label value` pair under the row title. */
  export interface TimelineFact {
    /** i18n key of the fact label. */
    labelKey: string;
    /** Already formatted value. */
    value: string;
  }

  /** Render-ready projection of one timeline event. */
  export interface TaskTimelineRow {
    tone: TimelineRowTone;
    /** i18n key of the row title. */
    titleKey: string;
    /** Interpolation values for `titleKey`. */
    titleValues: Record<string, string>;
    /** Truncated payload the runtime attached to the event, when any. */
    preview: string | null;
    facts: TimelineFact[];
    /**
     * ISO 8601 timestamp. The runtime derives the terminal `task_completed`
     * timestamp from the last sibling event, so it is empty on a task whose
     * only recorded event is the completion itself.
     */
    timestamp: string;
    /** True when the event denotes a failure, which tints the row. */
    failed: boolean;
    /** True when the runtime cut the recorded payload before persisting it. */
    truncated: boolean;
  }

  /** Formats a millisecond duration the way the audit table does. */
  export function formatMs(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  /** Formats a USD cost, keeping sub-cent calls readable. */
  export function formatCost(usd: number): string {
    return usd < 0.01 ? `$${usd.toFixed(4)}` : `$${usd.toFixed(2)}`;
  }

  /**
   * Projects a wire event onto a render-ready row.
   *
   * Every branch is exhaustive over the union declared in `lib/ipc/tasks`, so a
   * new variant fails the type check here rather than rendering as a blank row.
   */
  export function toTimelineRow(event: TaskTimelineEvent): TaskTimelineRow {
    switch (event.type) {
      case "task_transition":
        return {
          tone: "task",
          titleKey: "observability.task_timeline_row_transition",
          titleValues: { status: event.status },
          preview: null,
          facts: [],
          timestamp: event.timestamp,
          failed: event.status === "failed",
          truncated: false,
        };

      case "step_started": {
        const facts: TimelineFact[] = [];
        if (event.tool !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_tool",
            value: event.tool,
          });
        }
        return {
          tone: "step",
          titleKey: "observability.task_timeline_row_step_started",
          titleValues: { step: event.step_id },
          preview: event.input_preview,
          facts,
          timestamp: event.timestamp,
          failed: false,
          truncated: false,
        };
      }

      case "step_completed": {
        const facts: TimelineFact[] = [];
        if (event.duration_ms !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_duration",
            value: formatMs(event.duration_ms),
          });
        }
        return {
          tone: "step",
          titleKey: event.success
            ? "observability.task_timeline_row_step_completed"
            : "observability.task_timeline_row_step_failed",
          titleValues: { step: event.step_id },
          preview: null,
          facts,
          timestamp: event.timestamp,
          failed: !event.success,
          truncated: false,
        };
      }

      case "llm_call": {
        const facts: TimelineFact[] = [
          {
            labelKey: "observability.task_timeline_fact_backend",
            value: event.backend,
          },
        ];
        if (event.prompt_tokens !== null || event.completion_tokens !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_tokens",
            value: `${event.prompt_tokens ?? 0} / ${event.completion_tokens ?? 0}`,
          });
        }
        if (event.cost_usd !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_cost",
            value: formatCost(event.cost_usd),
          });
        }
        if (event.latency_ms !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_latency",
            value: formatMs(event.latency_ms),
          });
        }
        return {
          tone: "llm",
          titleKey: "observability.task_timeline_row_llm_call",
          titleValues: { model: event.model },
          preview: null,
          facts,
          timestamp: event.timestamp,
          failed: false,
          truncated: false,
        };
      }

      case "tool_call": {
        const facts: TimelineFact[] = [];
        if (event.duration_ms !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_duration",
            value: formatMs(event.duration_ms),
          });
        }
        if (event.exit_code !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_exit_code",
            value: String(event.exit_code),
          });
        }
        const failed = event.exit_code !== null && event.exit_code !== 0;
        return {
          tone: "tool",
          titleKey: failed
            ? "observability.task_timeline_row_tool_call_failed"
            : "observability.task_timeline_row_tool_call",
          titleValues: { tool: event.tool_name },
          preview: event.output_preview ?? event.input_preview ?? null,
          facts,
          timestamp: event.timestamp,
          failed,
          truncated: event.truncated,
        };
      }

      case "hitl_suspended":
        return {
          tone: "hitl",
          titleKey: "observability.task_timeline_row_hitl_suspended",
          titleValues: {},
          preview: event.prompt,
          facts: [],
          timestamp: event.timestamp,
          failed: false,
          truncated: false,
        };

      case "hitl_resolved": {
        const facts: TimelineFact[] = [];
        if (event.wait_ms !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_wait",
            value: formatMs(event.wait_ms),
          });
        }
        return {
          tone: "hitl",
          titleKey: event.approved
            ? "observability.task_timeline_row_hitl_approved"
            : "observability.task_timeline_row_hitl_rejected",
          titleValues: {},
          preview: event.reason,
          facts,
          timestamp: event.timestamp,
          failed: !event.approved,
          truncated: false,
        };
      }

      case "task_completed": {
        const facts: TimelineFact[] = [];
        if (event.duration_ms !== null) {
          facts.push({
            labelKey: "observability.task_timeline_fact_duration",
            value: formatMs(event.duration_ms),
          });
        }
        return {
          tone: "task",
          titleKey: "observability.task_timeline_row_task_completed",
          titleValues: {},
          preview: event.output_preview,
          facts,
          timestamp: event.timestamp,
          failed: false,
          truncated: false,
        };
      }
    }
  }
</script>

<script lang="ts">
  /**
   * `TaskTimeline` - the aggregated execution timeline of a single task.
   *
   * Reads `get_task_timeline`, which merges five persisted SQLite sources
   * (task transitions, plan steps, model calls, tool invocations, HITL
   * approvals) into one list sorted oldest first. Complementary to the Trace
   * tab of a task, which replays the live `runtime_events` stream instead.
   */
  import { t } from "svelte-i18n";
  import {
    Bot,
    ClipboardList,
    Hand,
    ListChecks,
    Wrench,
    type Icon,
  } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  // Imported by path rather than through the `operator` barrel: the barrel
  // pulls browser-only stores, which breaks the node-side unit tests of the
  // projection exported by this module.
  import ErrorBanner from "$lib/components/operator/ErrorBanner.svelte";
  import SkeletonList from "$lib/components/operator/SkeletonList.svelte";
  import { getTaskTimeline } from "$lib/ipc/tasks";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";

  interface Props {
    /** Task whose timeline is read. */
    taskId: string;
  }

  let { taskId }: Props = $props();

  const TONE_ICON: Record<TimelineRowTone, typeof Icon> = {
    task: ClipboardList,
    step: ListChecks,
    tool: Wrench,
    llm: Bot,
    hitl: Hand,
  };

  const TONE_COLOR: Record<TimelineRowTone, string> = {
    task: "hsl(var(--info))",
    step: "hsl(var(--primary))",
    tool: "hsl(var(--warning))",
    llm: "hsl(var(--chart-6))",
    hitl: "hsl(var(--destructive))",
  };

  let events = $state<TaskTimelineEvent[]>([]);
  let loading = $state(true);
  let errState = $state<HumanizedError | null>(null);

  const rows = $derived(events.map(toTimelineRow));

  async function load(id: string): Promise<void> {
    loading = true;
    try {
      events = await getTaskTimeline(id);
      errState = null;
    } catch (err: unknown) {
      events = [];
      errState = reportError(err, { surface: "inline" });
    } finally {
      loading = false;
    }
  }

  // Re-reads whenever the selected task changes, including the first render.
  $effect(() => {
    void load(taskId);
  });

  function formatTimestamp(iso: string): string {
    if (!iso) return "";
    return new Date(iso).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }
</script>

<div class="space-y-4" data-testid="task-timeline">
  <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
    <h3 class="m-0 text-body-sm font-semibold text-foreground">
      {$t("observability.task_timeline_heading")}
    </h3>
    <span class="section-meta tabular-nums" data-testid="task-timeline-count">
      {$t("observability.task_timeline_count", { values: { n: rows.length } })}
    </span>
    <p class="w-full text-caption text-muted-foreground">
      {$t("observability.task_timeline_sources")}
    </p>
  </div>

  {#if loading}
    <SkeletonList count={5} avatar={false} rowClass="py-1" />
  {:else if errState}
    <ErrorBanner
      message={errState.friendly_message}
      onretry={() => void load(taskId)}
      retryLabel={$t("common.retry")}
      data-testid="task-timeline-error"
    />
  {:else if rows.length === 0}
    <Card class="flex flex-col items-center justify-center py-12" data-testid="task-timeline-empty">
      <p class="text-body-sm text-muted-foreground">
        {$t("observability.task_timeline_empty")}
      </p>
    </Card>
  {:else}
    <Card class="divide-y divide-border/30 overflow-hidden">
      {#each rows as row, index (`${row.titleKey}-${row.timestamp}-${index}`)}
        {@const RowIcon = TONE_ICON[row.tone]}
        <div
          class="flex items-start gap-3 px-4 py-3"
          data-testid="task-timeline-row"
          data-tone={row.tone}
        >
          <div class="flex flex-col items-center pt-0.5 flex-shrink-0">
            <span
              class="inline-block h-2 w-2 rounded-full"
              style="background-color: {TONE_COLOR[row.tone]}"
              aria-hidden="true"
            ></span>
            <RowIcon
              class="mt-1.5 h-3.5 w-3.5"
              style="color: {TONE_COLOR[row.tone]};"
              aria-hidden="true"
            />
          </div>

          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <span
                class="text-body-sm font-medium"
                class:text-destructive={row.failed}
                class:text-foreground={!row.failed}
              >
                {$t(row.titleKey, { values: row.titleValues })}
              </span>
              {#if row.timestamp}
                <span class="text-caption tabular-nums text-muted-foreground">
                  {formatTimestamp(row.timestamp)}
                </span>
              {/if}
            </div>

            {#if row.facts.length > 0}
              <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1">
                {#each row.facts as fact (fact.labelKey)}
                  <span class="text-caption text-muted-foreground">
                    <span class="text-muted-foreground/60">{$t(fact.labelKey)}</span>
                    <span class="ml-1 tabular-nums text-foreground/80">{fact.value}</span>
                  </span>
                {/each}
              </div>
            {/if}

            {#if row.preview}
              <pre
                class="mt-2 overflow-x-auto rounded-md glass-inset border border-border/30 p-2 text-code-sm font-mono leading-relaxed text-muted-foreground">{row.preview}</pre>
            {/if}
          </div>

          <div class="flex flex-shrink-0 items-center gap-1.5">
            {#if row.truncated}
              <Badge variant="outline" size="sm">{$t("tasks.truncated")}</Badge>
            {/if}
            {#if row.failed}
              <Badge variant="danger" size="sm">
                {$t("observability.task_timeline_failed")}
              </Badge>
            {/if}
          </div>
        </div>
      {/each}
    </Card>
  {/if}
</div>
