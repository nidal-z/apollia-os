<script lang="ts">
  /**
   * BatchHistoryRow - one batch of the history on one line: when it was
   * launched, by which agent and on what, how far it went, how long it took
   * and how it ended. The line opens the batch; the icon at its end deletes
   * it, once it has ended.
   */
  import { locale, t } from "svelte-i18n";
  import { BookOpen, ChevronRight } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import type { BatchSummary } from "$lib/ipc/batches";
  import BatchDeleteAction from "./BatchDeleteAction.svelte";
  import { OUTCOME_TONE, durationMinutes, isLive, outcomeOf, progressOf, sourceName } from "./batchView";

  interface Props {
    batch: BatchSummary;
    now: Date;
    onOpen: (batch: BatchSummary) => void;
    onDeleted: (batchId: string) => void;
  }

  let { batch, now, onOpen, onDeleted }: Props = $props();

  const outcome = $derived(outcomeOf(batch));
  const progress = $derived(progressOf(batch));
  const minutes = $derived(durationMinutes(batch, now));

  const title = $derived.by(() => {
    const date = new Date(batch.createdAt);
    if (Number.isNaN(date.getTime())) return batch.batchId;
    const day = date.toLocaleDateString($locale ?? "en", { day: "numeric", month: "long" });
    const hour = date.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit" });
    return $t("batch_view.title", { values: { day, hour } });
  });

  const counters = $derived.by(() => {
    if (isLive(batch)) {
      return [$t("batch_history.treated", { values: { treated: progress.treated, total: progress.total } })];
    }
    return [
      $t("batch_view.count.completed", { values: { count: batch.completed } }),
      batch.failed > 0 ? $t("batch_view.count.failed", { values: { count: batch.failed } }) : "",
      batch.awaiting > 0 ? $t("batch_view.count.awaiting", { values: { count: batch.awaiting } }) : "",
      batch.canceled > 0 ? $t("batch_view.count.canceled", { values: { count: batch.canceled } }) : "",
    ].filter((part) => part !== "");
  });
</script>

<li class="flex items-center gap-2 rounded-lg border border-border bg-card pr-2 transition-colors hover:bg-muted/40">
  <button
    type="button"
    class="flex min-w-0 flex-1 items-center gap-4 rounded-lg px-4 py-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
    onclick={() => onOpen(batch)}
    data-testid="batch-history-row"
    data-batch={batch.batchId}
    data-outcome={outcome}
  >
    <BookOpen size={16} strokeWidth={1.5} class="shrink-0 text-muted-foreground" aria-hidden="true" />
    <span class="min-w-0 flex-[2]">
      <span class="block text-body-sm font-medium text-foreground">{title}</span>
      <span class="block truncate text-caption text-muted-foreground">
        {batch.agent} · {sourceName(batch.source.location)} ·
        {$t("batch_history.items", { values: { count: batch.total } })}
      </span>
    </span>
    <span class="min-w-0 flex-[2] text-body-sm text-foreground" data-testid="batch-history-counters">
      {counters.join(" · ")}
    </span>
    <span class="w-16 shrink-0 text-caption text-muted-foreground">
      {#if minutes !== null}{$t("batch_history.minutes", { values: { count: minutes } })}{/if}
    </span>
    <span class="w-44 shrink-0">
      <Badge variant={OUTCOME_TONE[outcome]} outline>
        {$t(`batch_history.outcome.${outcome}`, { values: { count: batch.failed } })}
      </Badge>
    </span>
    <ChevronRight size={16} strokeWidth={1.5} class="shrink-0 text-muted-foreground" aria-hidden="true" />
  </button>
  <BatchDeleteAction {batch} compact ondeleted={onDeleted} data-testid="batch-history-delete" />
</li>
