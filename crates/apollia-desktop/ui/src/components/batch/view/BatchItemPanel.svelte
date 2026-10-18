<script lang="ts">
  /**
   * BatchItemPanel - the item open beside the list of a batch. Its head says
   * the item, its state, how long it took or has been running and where that
   * time went: the plan, the steps, the plans redone. Below, the course of its
   * run as the conversation reads a turn, then its answer, or the cause of its
   * failure in plain words with the raw text at hand; its input and the
   * sources it consulted are folded.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { MarkdownContent } from "$lib/components/ui/markdown";
  import { Spinner } from "$lib/components/ui/progress";
  import type { BatchItem } from "$lib/ipc/batches";
  import { parseTimestampMs } from "$lib/utils";
  import BatchItemActivity from "./BatchItemActivity.svelte";
  import { ITEM_TONE, awaitedAccess, citedSources, entryOf, fieldsOf, statusOf } from "./batchView";
  import { failureText } from "./failureText";
  import type { ItemActivity } from "./itemActivity";
  import { elapsedLabel, secondsSince } from "./liveSteps";

  interface Props {
    item: BatchItem;
    activity: ItemActivity;
    /** The activity could not be read from the record. */
    unread: boolean;
    /** What the agent is doing now, said as the list says it. */
    liveLine: string | null;
    /** The clock running durations are read on, in ms. */
    now: number;
  }

  let { item, activity, unread, liveLine, now }: Props = $props();

  const status = $derived(statusOf(item));
  const fields = $derived(fieldsOf(item));
  const cited = $derived([item.origin, ...citedSources(item)].filter((name) => name !== ""));
  const running = $derived(status === "running");

  const took = $derived.by(() => {
    if (running && item.updatedAt !== undefined) {
      return elapsedLabel(secondsSince(parseTimestampMs(item.updatedAt), now));
    }
    return typeof item.durationMs === "number" && item.durationMs > 0
      ? elapsedLabel(Math.round(item.durationMs / 1000))
      : "";
  });

  // Where the time of the run went, read on its activity.
  const times = $derived.by(() => {
    let plan = 0;
    let steps = 0;
    let replans = 0;
    for (const entry of activity.entries) {
      if (entry.kind === "planning" && entry.durationMs !== null) plan += entry.durationMs;
      if (entry.kind === "step" && entry.durationMs !== null) steps += entry.durationMs;
      if (entry.kind === "replan") replans += 1;
    }
    const parts: string[] = [];
    if (plan >= 1000) parts.push($t("batch_view.times.plan", { values: { time: elapsedLabel(Math.round(plan / 1000)) } }));
    if (steps >= 1000) parts.push($t("batch_view.times.steps", { values: { time: elapsedLabel(Math.round(steps / 1000)) } }));
    if (replans > 0) parts.push($t("batch_view.times.replans", { values: { count: replans } }));
    return parts;
  });

  const failure = $derived(status === "failed" && item.output !== null ? failureText(item.output, $t) : null);
</script>

<section
  class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-surface-1"
  aria-label={$t("batch_view.item.panel_label", { values: { number: item.rank + 1 } })}
  data-testid="batch-item-panel"
  data-rank={item.rank}
>
  <header class="shrink-0 space-y-2 border-b border-border px-5 py-4">
    <h3 class="m-0 break-words text-body font-medium text-foreground">
      {$t("batch_view.item.heading", { values: { number: item.rank + 1, title: entryOf(item).title } })}
    </h3>
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-body-sm">
      <Badge variant={ITEM_TONE[status]} outline>{$t(`batch_view.item_status.${status}`)}</Badge>
      {#if took !== ""}
        <span class="tabular-nums text-muted-foreground" data-testid="batch-item-took">{took}</span>
      {/if}
      {#if running && liveLine !== null}
        <span class="flex min-w-0 items-center gap-1.5 text-foreground" data-testid="batch-item-panel-live">
          <Spinner size={12} class="shrink-0" />
          <span class="break-words">{liveLine}</span>
        </span>
      {/if}
    </div>
    {#if times.length > 0}
      <p class="m-0 text-caption text-muted-foreground" data-testid="batch-item-times">{times.join(" · ")}</p>
    {/if}
  </header>

  <div class="min-h-0 flex-1 space-y-5 overflow-y-auto px-5 py-4">
    <section class="space-y-2" data-testid="batch-item-detail-steps">
      <h4 class="m-0 text-caption font-medium uppercase tracking-wide text-muted-foreground">
        {$t("batch_view.activity.title")}
      </h4>
      {#if unread}
        <p class="m-0 text-body-sm text-muted-foreground">{$t("batch_view.item.steps_unread")}</p>
      {/if}
      <BatchItemActivity
        {activity}
        {running}
        sessionId={item.taskId}
        ran={item.taskId !== null || status !== "pending"}
      />
    </section>

    {#if status !== "pending" && status !== "running"}
      <section class="space-y-2 border-t border-border pt-4" data-testid="batch-item-detail-output">
        <h4 class="m-0 text-caption font-medium uppercase tracking-wide text-muted-foreground">
          {$t(status === "failed" ? "batch_view.item.failure" : "batch_view.item.output")}
        </h4>
        {#if status === "awaiting"}
          {@const access = awaitedAccess(item)}
          <p class="m-0 text-body-sm text-foreground">
            {access === null
              ? $t("batch_view.output.awaiting")
              : $t("batch_view.output.awaiting_access", { values: { access } })}
          </p>
        {:else if status === "canceled"}
          <p class="m-0 text-body-sm text-foreground">{$t("batch_view.output.canceled")}</p>
        {:else if failure !== null}
          <p class="m-0 break-words text-body-sm text-danger-a11y" data-testid="batch-item-failure">{failure.text}</p>
          {#if failure.hint !== null}
            <p class="m-0 break-words text-body-sm text-foreground">{failure.hint}</p>
          {/if}
          {#if failure.text !== failure.raw}
            <details class="text-caption text-muted-foreground">
              <summary class="cursor-pointer">{$t("batch_view.failure.raw")}</summary>
              <p class="m-0 mt-1 whitespace-pre-wrap break-words font-mono">{failure.raw}</p>
            </details>
          {/if}
        {:else if item.output !== null}
          <div class="text-body-sm text-foreground" data-testid="batch-item-output-text">
            <MarkdownContent content={item.output} />
          </div>
        {:else}
          <p class="m-0 text-body-sm text-muted-foreground">{$t("batch_view.item.no_output")}</p>
        {/if}
      </section>
    {/if}

    <details class="border-t border-border pt-4" data-testid="batch-item-detail-input">
      <summary class="cursor-pointer text-caption font-medium uppercase tracking-wide text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        {$t("batch_view.item.input_count", { values: { count: fields.length } })}
      </summary>
      {#if fields.length === 0}
        <p class="m-0 mt-2 whitespace-pre-wrap break-words text-body-sm text-foreground">{entryOf(item).title}</p>
      {:else}
        <dl class="m-0 mt-3 grid grid-cols-1 gap-x-4 gap-y-2.5 sm:grid-cols-[minmax(7rem,auto)_1fr]">
          {#each fields as field (field.name)}
            <dt class="text-caption text-muted-foreground sm:pt-0.5">{field.name}</dt>
            <dd class="m-0 whitespace-pre-wrap break-words text-body-sm text-foreground">{field.value}</dd>
          {/each}
        </dl>
      {/if}
    </details>

    <details class="border-t border-border pt-4" data-testid="batch-item-detail-sources">
      <summary class="cursor-pointer text-caption font-medium uppercase tracking-wide text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        {$t("batch_view.item.sources", { values: { count: cited.length } })}
      </summary>
      <ul class="m-0 mt-2 list-none space-y-1 p-0">
        {#each cited as source (source)}
          <li class="break-words text-body-sm text-foreground">{source}</li>
        {/each}
      </ul>
    </details>
  </div>
</section>
