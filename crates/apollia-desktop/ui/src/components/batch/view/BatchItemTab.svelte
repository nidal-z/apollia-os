<script lang="ts">
  /**
   * BatchItemTab - the "Item" tab of the detail sidebar: the item of a batch
   * the user opened, with its whole input, its output or the cause of its
   * failure, the steps of its run and the sources it consulted.
   */
  import { locale, t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import type { TaskTimelineEvent } from "$lib/ipc/tasks";
  import { batchItemSelection } from "$lib/stores/batchView";
  import { ITEM_TONE, awaitedAccess, citedSources, entryOf, fieldsOf, statusOf } from "./batchView";

  function hour(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  /** What a step names: its tool, its step, or nothing more than its kind. */
  function stepName(step: TaskTimelineEvent): string {
    const named = step as unknown as Record<string, unknown>;
    const name = ["tool_name", "tool", "step_id"].find((key) => typeof named[key] === "string");
    return name === undefined ? "" : (named[name] as string);
  }
</script>

{#if $batchItemSelection !== null}
  {@const item = $batchItemSelection.item}
  {@const status = statusOf(item)}
  {@const fields = fieldsOf(item)}
  {@const cited = [item.origin, ...citedSources(item)].filter((name) => name !== "")}
  {@const steps = $batchItemSelection.steps}
  <div class="flex h-full flex-col gap-3 overflow-y-auto p-4" data-testid="detail-sidebar-item" data-rank={item.rank}>
    <h3 class="m-0 break-words text-body font-medium text-foreground">
      {$t("batch_view.item.heading", { values: { number: item.rank + 1, title: entryOf(item).title } })}
    </h3>
    <div>
      <Badge variant={ITEM_TONE[status]} outline>{$t(`batch_view.item_status.${status}`)}</Badge>
    </div>

    <details open class="border-t border-border pt-3" data-testid="batch-item-detail-input">
      <summary class="cursor-pointer text-body-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        {$t("batch_view.item.input")}
      </summary>
      {#if fields.length === 0}
        <p class="m-0 mt-2 whitespace-pre-wrap break-words text-body-sm text-foreground">{entryOf(item).title}</p>
      {:else}
        <dl class="m-0 mt-2 space-y-2">
          {#each fields as field (field.name)}
            <div>
              <dt class="text-caption text-muted-foreground">{field.name}</dt>
              <dd class="m-0 whitespace-pre-wrap break-words text-body-sm text-foreground">{field.value}</dd>
            </div>
          {/each}
        </dl>
      {/if}
    </details>

    <details open class="border-t border-border pt-3" data-testid="batch-item-detail-output">
      <summary class="cursor-pointer text-body-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        {$t(status === "failed" ? "batch_view.item.failure" : "batch_view.item.output")}
      </summary>
      <p class="m-0 mt-2 whitespace-pre-wrap break-words text-body-sm {status === 'failed' ? 'text-danger-a11y' : 'text-foreground'}">
        {#if status === "awaiting"}
          {@const access = awaitedAccess(item)}
          {access === null
            ? $t("batch_view.output.awaiting")
            : $t("batch_view.output.awaiting_access", { values: { access } })}
        {:else if status === "canceled"}
          {$t("batch_view.output.canceled")}
        {:else}
          {item.output ?? $t("batch_view.item.no_output")}
        {/if}
      </p>
    </details>

    <details class="border-t border-border pt-3" data-testid="batch-item-detail-steps">
      <summary class="cursor-pointer text-body-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        {$t("batch_view.item.steps", { values: { count: steps.status === "ready" ? steps.steps.length : 0 } })}
      </summary>
      {#if steps.status === "loading"}
        <p class="m-0 mt-2 text-caption text-muted-foreground">{$t("common.loading")}</p>
      {:else if steps.status === "error"}
        <p class="m-0 mt-2 text-caption text-muted-foreground">{$t("batch_view.item.steps_unread")}</p>
      {:else if steps.status === "none" || steps.steps.length === 0}
        <p class="m-0 mt-2 text-caption text-muted-foreground">{$t("batch_view.item.no_step")}</p>
      {:else}
        <ol class="m-0 mt-2 list-none space-y-1.5 p-0">
          {#each steps.steps as step, index (index)}
            <li class="text-caption text-muted-foreground">
              <span class="text-foreground">{$t(`batch_view.step.${step.type}`)}</span>
              {#if stepName(step) !== ""}· {stepName(step)}{/if}
              · {hour(step.timestamp)}
            </li>
          {/each}
        </ol>
      {/if}
    </details>

    <details class="border-t border-border pt-3" data-testid="batch-item-detail-sources">
      <summary class="cursor-pointer text-body-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        {$t("batch_view.item.sources", { values: { count: cited.length } })}
      </summary>
      <ul class="m-0 mt-2 list-none space-y-1 p-0">
        {#each cited as source (source)}
          <li class="break-words text-caption text-muted-foreground">{source}</li>
        {/each}
      </ul>
    </details>
  </div>
{/if}
