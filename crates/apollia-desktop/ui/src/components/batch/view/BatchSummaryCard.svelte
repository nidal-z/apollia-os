<script lang="ts">
  /**
   * BatchSummaryCard - the bar that stays at the top of the view of a batch:
   * its state, the items dealt with out of the total on one line with the
   * progress, the counters and the commands; then the item running and what
   * its agent does, the pace kept and the end it gives, and the settings of
   * the batch folded.
   */
  import { locale, t } from "svelte-i18n";
  import { parseTimestampMs } from "$lib/utils";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { ProgressBar } from "$lib/components/ui/progress";
  import { Card } from "$lib/components/operator";
  import type { BatchControl, BatchExportFormat, BatchSummary } from "$lib/ipc/batches";
  import BatchExportMenu from "./BatchExportMenu.svelte";
  import {
    BAR_VARIANT,
    STATE_TONE,
    commandsFor,
    durationMinutes,
    progressOf,
    stateOf,
    writtenColumns,
  } from "./batchView";

  interface Props {
    batch: BatchSummary;
    /** The command being applied, `null` when none is. */
    busy: BatchControl | "export" | null;
    /** What each access granted lets the agent do, by access. */
    accessLabels: Record<string, string>;
    /** The time the schedule is computed against. */
    now: Date;
    onAct: (action: BatchControl) => void;
    onExport: (format: BatchExportFormat) => void;
    /** The item running now and what its agent does, `null` when none runs. */
    current?: { number: number; line: string } | null;
    /** The batch launched earlier that runs before this one, when it waits for it. */
    waitingFor?: { agent: string; started: string } | null;
    /** The time an item takes, read on the last items that ended, `null` before any did. */
    perItemMs?: number | null;
  }

  let {
    batch,
    busy,
    accessLabels,
    now,
    onAct,
    onExport,
    current = null,
    waitingFor = null,
    perItemMs = null,
  }: Props = $props();

  /** A maximum the launch dialog sends for "never interrupt". */
  const NEVER_INTERRUPT = 4_294_967_295;

  const state = $derived(stateOf(batch));
  const progress = $derived(progressOf(batch));
  const commands = $derived(commandsFor(batch));
  const columns = $derived(writtenColumns(batch));
  const live = $derived(state === "running" || state === "paused");

  function hour(value: string | Date | null): string {
    if (value === null) return "";
    const date = typeof value === "string" ? new Date(parseTimestampMs(value)) : value;
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit" });
  }

  const schedule = $derived.by(() => {
    const started = hour(batch.createdAt);
    const values = { started, at: hour(batch.endedAt ?? batch.updatedAt), minutes: durationMinutes(batch, now) ?? 0 };
    if (state === "running") {
      // At the pace of the last items that ended, pauses left out: the time
      // since the start counted every pause, and a batch resumed two days
      // later said it would end at night.
      if (perItemMs === null || progress.remaining === 0) return $t("batch_view.schedule.started", { values });
      const end = new Date(now.getTime() + perItemMs * progress.remaining);
      return $t("batch_view.schedule.pace", {
        values: { started, per: paceLabel(perItemMs), end: hour(end) },
      });
    }
    return $t(`batch_view.schedule.${state}`, { values });
  });

  /** A duration read at a glance: seconds, then minutes and seconds. */
  function paceLabel(ms: number): string {
    const seconds = Math.round(ms / 1000);
    if (seconds < 60) return `${seconds} s`;
    const rest = seconds % 60;
    return rest === 0 ? `${Math.floor(seconds / 60)} min` : `${Math.floor(seconds / 60)} min ${rest} s`;
  }

  const message = $derived.by(() => {
    const values = { remaining: progress.remaining, canceled: batch.canceled, failures: batch.maxFailures };
    if (state === "paused") return $t("batch_view.message.paused");
    if (state === "interrupted") return $t("batch_view.message.interrupted", { values });
    if (state === "stopped") return $t("batch_view.message.stopped", { values });
    return "";
  });

  const grants = $derived(batch.grants.map((grant) => accessLabels[grant] ?? grant));

  const facts = $derived(
    [
      batch.maxFailures >= NEVER_INTERRUPT
        ? $t("batch_view.fact.threshold_never")
        : $t("batch_view.fact.threshold", { values: { count: batch.maxFailures } }),
      grants.length === 0
        ? $t("batch_view.fact.no_access")
        : $t(live ? "batch_view.fact.accesses" : "batch_view.fact.accesses_ended", {
            values: { accesses: grants.join(", ") },
          }),
      columns.length === 0
        ? ""
        : $t(`batch_view.fact.written_${batch.source.write?.destination ?? "copy"}`, {
            values: { columns: columns.join(", ") },
          }),
    ].filter((fact) => fact !== ""),
  );
</script>

<Card class="sticky top-0 z-sticky p-4" data-testid="batch-summary">
  <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
    <Badge variant={STATE_TONE[state]} outline data-testid="batch-state" data-state={state}>
      {$t(`batch_view.state.${state}`)}
    </Badge>
    <p class="m-0 shrink-0 text-body font-medium tabular-nums text-foreground" data-testid="batch-treated">
      {$t("batch_view.treated_short", { values: { treated: progress.treated, total: progress.total } })}
    </p>
    <div
      class="min-w-40 flex-1"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={progress.total}
      aria-valuenow={progress.treated}
      aria-label={$t("batch_view.treated", { values: { treated: progress.treated, total: progress.total } })}
      data-testid="batch-progress"
      data-treated={progress.treated}
      data-total={progress.total}
    >
      <ProgressBar value={progress.percent} variant={BAR_VARIANT[state]} size="sm" />
    </div>
    <div class="flex shrink-0 items-baseline gap-3 text-body-sm tabular-nums">
      <span class="text-success-a11y" data-testid="batch-count-completed">
        {$t("batch_view.count.completed", { values: { count: batch.completed } })}
      </span>
      {#if batch.failed > 0}
        <span class="text-danger-a11y" data-testid="batch-count-failed">
          {$t("batch_view.count.failed", { values: { count: batch.failed } })}
        </span>
      {/if}
      {#if batch.awaiting > 0}
        <span class="text-muted-foreground" data-testid="batch-count-awaiting">
          {$t("batch_view.count.awaiting", { values: { count: batch.awaiting } })}
        </span>
      {/if}
      {#if progress.remaining > 0}
        <span class="text-muted-foreground" data-testid="batch-count-todo">
          {$t("batch_view.count.todo", { values: { count: progress.remaining } })}
        </span>
      {/if}
      {#if batch.canceled > 0}
        <span class="text-muted-foreground" data-testid="batch-count-canceled">
          {$t("batch_view.count.canceled", { values: { count: batch.canceled } })}
        </span>
      {/if}
    </div>
    <div class="flex flex-wrap items-center gap-2">
      {#each commands as command (command.id)}
        {#if command.id === "export"}
          <BatchExportMenu disabled={busy !== null} loading={busy === "export"} {onExport} />
        {:else}
          {@const action = command.id}
          <Button
            variant={command.primary ? "primary-solid" : "outline"}
            size="sm"
            disabled={!command.enabled || busy !== null}
            loading={busy === action}
            onclick={() => onAct(action)}
            data-testid={`batch-${action}`}
          >
            {$t(`batch_view.command.${action}`)}
          </Button>
        {/if}
      {/each}
    </div>
  </div>

  <div class="mt-2 flex flex-wrap items-baseline gap-x-4 gap-y-1 text-caption">
    {#if current !== null}
      <span class="min-w-0 break-words text-foreground" data-testid="batch-current">
        {$t("batch_view.current", { values: { number: current.number, line: current.line } })}
      </span>
    {:else if waitingFor !== null}
      <span class="text-warning-a11y" data-testid="batch-waiting">
        {$t("batch_view.waiting_for", { values: { agent: waitingFor.agent, started: hour(waitingFor.started) } })}
      </span>
    {/if}
    {#if message !== ""}
      <span class={state === "interrupted" ? "text-danger-a11y" : "text-foreground"} data-testid="batch-message">
        {message}
      </span>
    {/if}
    <span class="ml-auto text-muted-foreground" data-testid="batch-schedule">{schedule}</span>
    <details class="w-full text-muted-foreground" data-testid="batch-facts">
      <summary class="cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        {$t("batch_view.settings")}
      </summary>
      <p class="m-0 mt-1">{facts.join(" · ")}</p>
    </details>
  </div>
</Card>
