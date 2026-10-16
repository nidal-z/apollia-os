<script lang="ts">
  /**
   * BatchItemPanel - one item of a batch, opened beside the table: what its
   * agent is doing now, the whole course of its run read as the conversation
   * reads a turn, its answer or the cause of its failure, its whole input and
   * the sources it consulted. Wide enough for the input to read as a sheet,
   * not as a column of words.
   */
  import { t } from "svelte-i18n";
  import { X } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { MarkdownContent } from "$lib/components/ui/markdown";
  import { Spinner } from "$lib/components/ui/progress";
  import type { BatchItem } from "$lib/ipc/batches";
  import BatchItemActivity from "./BatchItemActivity.svelte";
  import { ITEM_TONE, awaitedAccess, citedSources, entryOf, fieldsOf, statusOf } from "./batchView";
  import type { ItemActivity } from "./itemActivity";

  interface Props {
    item: BatchItem;
    activity: ItemActivity;
    /** The activity could not be read from the record. */
    unread: boolean;
    /** What the agent is doing now, said as the table says it. */
    liveLine: string | null;
    onclose: () => void;
  }

  let { item, activity, unread, liveLine, onclose }: Props = $props();

  const status = $derived(statusOf(item));
  const fields = $derived(fieldsOf(item));
  const cited = $derived([item.origin, ...citedSources(item)].filter((name) => name !== ""));
  const running = $derived(status === "running");
</script>

<section
  class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-surface-1"
  aria-label={$t("batch_view.item.panel_label", { values: { number: item.rank + 1 } })}
  data-testid="batch-item-panel"
  data-rank={item.rank}
>
  <header class="flex shrink-0 items-start gap-3 border-b border-border px-5 py-4">
    <div class="min-w-0 flex-1 space-y-1.5">
      <h3 class="m-0 break-words text-body font-medium text-foreground">
        {$t("batch_view.item.heading", { values: { number: item.rank + 1, title: entryOf(item).title } })}
      </h3>
      <div class="flex flex-wrap items-center gap-2">
        <Badge variant={ITEM_TONE[status]} outline>{$t(`batch_view.item_status.${status}`)}</Badge>
        {#if running && liveLine !== null}
          <span class="flex min-w-0 items-center gap-1.5 text-body-sm text-foreground" data-testid="batch-item-panel-live">
            <Spinner size={12} class="shrink-0" />
            <span class="break-words">{liveLine}</span>
          </span>
        {/if}
      </div>
    </div>
    <Button
      variant="ghost"
      size="icon-sm"
      onclick={onclose}
      aria-label={$t("batch_view.item.close")}
      title={$t("batch_view.item.close")}
      data-testid="batch-item-panel-close"
    >
      <X size={14} strokeWidth={1.5} />
    </Button>
  </header>

  <div class="min-h-0 flex-1 space-y-5 overflow-y-auto px-5 py-4">
    <section class="space-y-2" data-testid="batch-item-detail-steps">
      <h4 class="m-0 text-caption font-medium uppercase tracking-wide text-muted-foreground">
        {$t("batch_view.activity.title")}
      </h4>
      {#if unread}
        <p class="m-0 text-body-sm text-muted-foreground">{$t("batch_view.item.steps_unread")}</p>
      {/if}
      <BatchItemActivity {activity} {running} sessionId={item.taskId} ran={item.taskId !== null || status !== "pending"} />
    </section>

    {#if status !== "pending" && status !== "running"}
      <section class="space-y-2 border-t border-border pt-4" data-testid="batch-item-detail-output">
        <h4 class="m-0 text-caption font-medium uppercase tracking-wide text-muted-foreground">
          {$t(status === "failed" ? "batch_view.item.failure" : "batch_view.item.output")}
        </h4>
        <p class="m-0 whitespace-pre-wrap break-words text-body-sm {status === 'failed' ? 'text-danger-a11y' : 'text-foreground'}">
          {#if status === "awaiting"}
            {@const access = awaitedAccess(item)}
            {access === null
              ? $t("batch_view.output.awaiting")
              : $t("batch_view.output.awaiting_access", { values: { access } })}
          {:else if status === "canceled"}
            {$t("batch_view.output.canceled")}
          {:else if status === "failed" || item.output === null}
            {item.output ?? $t("batch_view.item.no_output")}
          {/if}
        </p>
        {#if status !== "failed" && status !== "awaiting" && status !== "canceled" && item.output !== null}
          <div class="text-body-sm text-foreground" data-testid="batch-item-output-text">
            <MarkdownContent content={item.output} />
          </div>
        {/if}
      </section>
    {/if}

    <details open class="border-t border-border pt-4" data-testid="batch-item-detail-input">
      <summary class="cursor-pointer text-caption font-medium uppercase tracking-wide text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        {$t("batch_view.item.input")}
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
