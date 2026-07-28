<script lang="ts">
  /**
   * TasksDetailPanes - tab bodies for a selected task.
   *
   * Overview / Output / Trace are operator-facing; Technical is builder-only.
   * The running task is the expressive focal point: its live card carries the
   * signature primary halo and a streaming cursor. Output keeps the two layers
   * (SmartOutput prose for operator, raw handled inside SmartOutput / error
   * mono block), and Trace defers to ExecutionTrace's own operator/builder split.
   */
  import { t } from "svelte-i18n";
  import { XCircle } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import StatusDot from "$lib/components/operator/StatusDot.svelte";
  import type { TaskSummary } from "$lib/types";
  import ExecutionTrace from "../observability/ExecutionTrace.svelte";
  import SmartOutput from "../common/SmartOutput.svelte";
  import StreamingCursor from "../chat/StreamingCursor.svelte";
  import { formatDurationLong, formatDate } from "./taskFormat";

  interface Props {
    task: TaskSummary;
    activeTab: string;
    isRunning: boolean;
    onviewoutput: () => void;
  }

  let { task, activeTab, isRunning, onviewoutput }: Props = $props();

  const TRUNCATION_MARKER = "[TRONQUE]";
  const inputTruncated = $derived(task.input_preview?.includes(TRUNCATION_MARKER) ?? false);
  const outputTruncated = $derived(task.output_text?.includes(TRUNCATION_MARKER) ?? false);
  const isFailed = $derived(task.status === "failed");
  const labelClass =
    "text-body-xs font-medium uppercase tracking-wider text-muted-foreground/50";
</script>

{#if activeTab === "overview"}
  <div class="space-y-3 max-w-3xl">
    <Card class="rounded-lg px-4 py-3.5" data-testid="task-overview-input">
      <div class="flex items-center gap-2 mb-2">
        <span class={labelClass}>{$t("tasks.input")}</span>
        {#if inputTruncated}
          <Badge variant="outline" size="sm">{$t("tasks.truncated")}</Badge>
        {/if}
      </div>
      <p class="whitespace-pre-wrap text-body-sm text-foreground/85 leading-relaxed">
        {task.input_preview || $t("common.no_input")}
      </p>
    </Card>

    {#if isRunning}
      <Card
        class="rounded-lg px-4 py-3.5 border-primary/35 glow-primary"
        data-testid="task-overview-live"
      >
        <div class="flex items-center gap-2 mb-2">
          <Badge variant="info" size="sm">
            {#snippet icon()}<StatusDot color="hsl(var(--primary))" glow />{/snippet}
            {$t("dashboard.status_working")}
          </Badge>
        </div>
        <p class="text-body-sm text-foreground/85 leading-relaxed">
          {$t("tasks.live_working_hint")}<StreamingCursor />
        </p>
      </Card>
    {:else if task.output_text}
      <Card
        class="rounded-lg px-4 py-3.5 {isFailed ? 'border-destructive/30' : ''}"
        data-testid="task-overview-output"
      >
        <div class="flex items-center gap-2 mb-2">
          {#if isFailed}
            <XCircle size={11} class="text-destructive shrink-0" />
            <span class="text-body-xs font-medium uppercase tracking-wider text-destructive/70">
              {$t("tasks.error")}
            </span>
          {:else}
            <span class={labelClass}>{$t("tasks.result")}</span>
          {/if}
          {#if outputTruncated}
            <Badge variant="outline" size="sm">{$t("tasks.truncated")}</Badge>
          {/if}
        </div>
        <div
          class="text-body-sm {isFailed
            ? 'text-destructive/80 font-mono break-all'
            : 'text-foreground/85 leading-relaxed'}"
        >
          {#if isFailed}
            {task.output_text}
          {:else}
            <p class="whitespace-pre-wrap">
              {task.output_text.length > 280
                ? task.output_text.slice(0, 280) + "…"
                : task.output_text}
            </p>
            {#if task.output_text.length > 280}
              <button
                type="button"
                class="mt-1.5 rounded text-body-xs text-primary hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/60"
                onclick={onviewoutput}
              >
                {$t("tasks.view_full_output")} →
              </button>
            {/if}
          {/if}
        </div>
      </Card>
    {/if}
  </div>
{:else if activeTab === "output"}
  <div class="max-w-3xl" data-testid="task-output-tab">
    {#if task.output_text}
      {#if isFailed}
        <Card class="rounded-lg px-4 py-3.5 border-destructive/30">
          <div class="flex items-center gap-2 mb-2">
            <XCircle size={11} class="text-destructive shrink-0" />
            <span class="text-body-xs font-medium uppercase tracking-wider text-destructive/70">
              {$t("tasks.error")}
            </span>
          </div>
          <pre
            class="whitespace-pre-wrap break-all text-body-sm text-destructive/80 font-mono">{task.output_text}</pre>
        </Card>
      {:else}
        <SmartOutput output={task.output_text} />
      {/if}
    {:else}
      <p class="text-body-sm text-muted-foreground italic">{$t("tasks.no_output")}</p>
    {/if}
  </div>
{:else if activeTab === "trace"}
  <div data-testid="task-trace-tab">
    <ExecutionTrace taskId={task.id} context="task" mode={isRunning ? "live" : "replay"} />
  </div>
{:else if activeTab === "technical"}
  <div class="max-w-3xl space-y-3" data-testid="task-technical-tab">
    <Card class="rounded-lg px-4 py-3.5">
      <div class="{labelClass} mb-2">{$t("tasks.tech_identifiers")}</div>
      <dl class="space-y-1.5 text-body-sm">
        <div class="flex items-baseline gap-3">
          <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.tech_task_id")}</dt>
          <dd><code class="font-mono text-foreground/85 break-all">{task.id}</code></dd>
        </div>
        <div class="flex items-baseline gap-3">
          <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.tech_agent_id")}</dt>
          <dd><code class="font-mono text-foreground/85">{task.agent_id}</code></dd>
        </div>
        <div class="flex items-baseline gap-3">
          <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.tech_agent_name")}</dt>
          <dd class="text-foreground/85">{task.agent_name || "-"}</dd>
        </div>
      </dl>
    </Card>

    <Card class="rounded-lg px-4 py-3.5">
      <div class="{labelClass} mb-2">{$t("tasks.tech_status_timing")}</div>
      <Separator class="mb-2.5" />
      <dl class="space-y-1.5 text-body-sm">
        <div class="flex items-baseline gap-3">
          <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.tech_raw_status")}</dt>
          <dd><code class="font-mono text-foreground/85">{task.status}</code></dd>
        </div>
        <div class="flex items-baseline gap-3">
          <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.created")}</dt>
          <dd class="text-foreground/85">{formatDate(task.created_at)}</dd>
        </div>
        <div class="flex items-baseline gap-3">
          <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.duration")}</dt>
          <dd class="text-foreground/85 tabular-nums">{formatDurationLong(task.duration_ms)}</dd>
        </div>
      </dl>
    </Card>
  </div>
{/if}
