<script lang="ts">
  /**
   * JournalEntryDetail - everything a journal entry records, shown on demand:
   * who, what, on what, when, with which result, then the parameters of the
   * action and what it returned. The developer mode adds the technical
   * details under it: identifiers, raw arguments, hashes.
   */
  import { locale, t } from "svelte-i18n";
  import { developerMode } from "$lib/stores/developerMode";
  import type { JournalLine } from "$lib/ipc/journal";
  import type { JournalDetailState } from "$lib/stores/journalSelection";
  import {
    STATE_LABEL,
    detailView,
    familyLabelKey,
    stateOf,
    technicalView,
    type NamedValue,
  } from "./journal";

  interface Props {
    line: JournalLine;
    /** Where the detail stands; `null` before it was asked for. */
    state: JournalDetailState | null;
    /** The name the agent of the entry is shown under. */
    agentName: string;
    /** Labels beside their values, or above them in a narrow column. */
    layout?: "beside" | "stacked";
  }

  let { line, state, agentName, layout = "beside" }: Props = $props();

  const ready = $derived(state?.status === "ready" ? state.detail : null);
  const view = $derived(ready === null ? null : detailView(ready));
  const technical = $derived(ready === null ? null : technicalView(ready));
  const lineState = $derived(stateOf(line));
  const familyKey = $derived(familyLabelKey(line.summary.family));

  const when = $derived.by(() => {
    const date = new Date(line.ts);
    if (Number.isNaN(date.getTime())) return line.ts;
    return date.toLocaleString($locale ?? "en", { dateStyle: "full", timeStyle: "medium" });
  });

  const grid = $derived(
    layout === "beside" ? "grid grid-cols-[10rem_minmax(0,1fr)] gap-x-4 gap-y-2" : "flex flex-col gap-1",
  );
  const term = $derived(
    layout === "beside" ? "text-caption text-muted-foreground" : "pt-2 text-caption text-muted-foreground",
  );
  const value = "m-0 min-w-0 break-words text-body-sm text-foreground";
</script>

{#snippet named(values: NamedValue[])}
  <ul class="m-0 flex list-none flex-col gap-1 p-0">
    {#each values as item (item.name)}
      <li class="flex min-w-0 flex-wrap gap-x-2 text-body-sm text-foreground">
        {#if item.name !== ""}<span class="text-muted-foreground">{item.name}</span>{/if}
        <span class="min-w-0 break-words">{item.value}</span>
      </li>
    {/each}
  </ul>
{/snippet}

<div class="flex flex-col gap-4" data-testid="journal-entry-detail">
  <dl class="m-0 {grid}">
    {#if line.summary.agent !== null}
      <dt class={term}>{$t("journal.detail.agent")}</dt>
      <dd class={value}>{agentName}</dd>
    {/if}
    {#if line.summary.tool !== null}
      <dt class={term}>{$t("journal.detail.action")}</dt>
      <dd class={value}>{line.summary.tool}</dd>
    {/if}
    {#if line.summary.target !== null}
      <dt class={term}>{$t("journal.detail.target")}</dt>
      <dd class={value} data-testid="journal-detail-target">{line.summary.target}</dd>
    {/if}
    {#if familyKey !== null}
      <dt class={term}>{$t("journal.detail.family")}</dt>
      <dd class={value}>{$t(familyKey)}</dd>
    {/if}
    <dt class={term}>{$t("journal.detail.when")}</dt>
    <dd class={value}>{when}</dd>
    <dt class={term}>{$t("journal.detail.result")}</dt>
    <dd class={value}>
      {lineState === null ? $t("journal.detail.in_progress") : $t(STATE_LABEL[lineState])}
    </dd>

    {#if state === null || state.status === "loading"}
      <dt class={term}>{$t("journal.detail.parameters")}</dt>
      <dd class="{value} text-muted-foreground">
        {$t("journal.detail.loading")}
      </dd>
    {:else if state.status === "missing" || state.status === "error"}
      <dt class={term}>{$t("journal.detail.parameters")}</dt>
      <dd class="{value} text-muted-foreground" data-testid="journal-detail-unavailable">
        {$t(state.status === "missing" ? "journal.detail.missing" : "journal.detail.failed")}
      </dd>
    {:else if view !== null && view.isCall}
      <dt class={term}>{$t("journal.detail.parameters")}</dt>
      <dd class={value} data-testid="journal-detail-parameters">
        {#if view.parameters === null}
          <span class="text-muted-foreground">{$t("journal.detail.parameters_none")}</span>
        {:else}
          {@render named(view.parameters)}
        {/if}
      </dd>
      <dt class={term}>{$t("journal.detail.output")}</dt>
      <dd class={value} data-testid="journal-detail-output">
        {#if view.output === null}
          <span class="text-muted-foreground">{$t("journal.detail.output_none")}</span>
        {:else}
          <pre class="m-0 max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted/50 p-2 font-mono text-caption text-foreground">{view.output}</pre>
        {/if}
      </dd>
    {:else if view !== null && view.record.length > 0}
      <dt class={term}>{$t("journal.detail.record")}</dt>
      <dd class={value} data-testid="journal-detail-record">{@render named(view.record)}</dd>
    {/if}
  </dl>

  {#if $developerMode && technical !== null}
    <section class="flex flex-col gap-2" data-testid="journal-detail-technical">
      <h4 class="m-0 text-caption font-medium text-muted-foreground">{$t("dev.journal.technical_title")}</h4>
      <dl class="m-0 {grid} font-mono text-caption">
        <dt class="text-muted-foreground">{$t("dev.journal.kind")}</dt>
        <dd class="m-0 min-w-0 break-all text-foreground">{technical.kind}</dd>
        {#if technical.tool !== null}
          <dt class="text-muted-foreground">{$t("dev.journal.tool")}</dt>
          <dd class="m-0 min-w-0 break-all text-foreground">{technical.tool}</dd>
        {/if}
        <dt class="text-muted-foreground">{$t("dev.journal.entry")}</dt>
        <dd class="m-0 min-w-0 break-all text-foreground">
          {$t("dev.journal.entry_value", { values: { seq: technical.seq } })}
        </dd>
        <dt class="text-muted-foreground">{$t("dev.journal.run")}</dt>
        <dd class="m-0 min-w-0 break-all text-foreground">{technical.runId}</dd>
        {#if technical.callId !== null}
          <dt class="text-muted-foreground">{$t("dev.journal.call")}</dt>
          <dd class="m-0 min-w-0 break-all text-foreground">{technical.callId}</dd>
        {/if}
        {#if technical.arguments !== ""}
          <dt class="text-muted-foreground">{$t("dev.journal.arguments")}</dt>
          <dd class="m-0 min-w-0">
            <pre class="m-0 max-h-64 overflow-auto whitespace-pre-wrap break-words text-foreground" data-testid="journal-detail-raw-arguments">{technical.arguments}</pre>
          </dd>
        {/if}
        <dt class="text-muted-foreground">{$t("dev.journal.payload")}</dt>
        <dd class="m-0 min-w-0">
          <pre class="m-0 max-h-64 overflow-auto whitespace-pre-wrap break-words text-foreground">{technical.payload}</pre>
        </dd>
        <dt class="text-muted-foreground">{$t("dev.journal.hash")}</dt>
        <dd class="m-0 min-w-0 break-all text-foreground">
          {$t("dev.journal.hash_value", { values: { hash: technical.hash, previous: technical.prevHash } })}
        </dd>
      </dl>
    </section>
  {/if}
</div>
