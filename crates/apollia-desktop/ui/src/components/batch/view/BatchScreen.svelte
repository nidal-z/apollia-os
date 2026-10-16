<script lang="ts">
  /**
   * BatchScreen - the view of one batch: its summary and commands, then its
   * table, which fills as the items end. The batch is read again whenever the
   * runtime reports a run or a decision, and at a slow pace besides, so the
   * figures shown are always those the batch engine recorded.
   */
  import { untrack } from "svelte";
  import { locale, t } from "svelte-i18n";
  import { parseTimestampMs } from "$lib/utils";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { Layers, Search } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { Input } from "$lib/components/ui/input";
  import { TabBar } from "$lib/components/ui/tabs";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Card, EmptyState, ErrorBanner, PageLayout, SkeletonList } from "$lib/components/operator";
  import { listBatchAccesses } from "$lib/ipc/batches";
  import type { BatchControl, BatchExportFormat, BatchItem } from "$lib/ipc/batches";
  import { getTaskTimeline } from "$lib/ipc/tasks";
  import { openBatchHistory, type BatchOrigin } from "$lib/stores/batchView";
  import BatchDecisionDialog from "./BatchDecisionDialog.svelte";
  import BatchDeleteAction from "./BatchDeleteAction.svelte";
  import BatchItemPanel from "./BatchItemPanel.svelte";
  import BatchItemsTable from "./BatchItemsTable.svelte";
  import BatchSummaryCard from "./BatchSummaryCard.svelte";
  import {
    exportFileName,
    filterItems,
    isLive,
    progressOf,
    shownItemTab,
    sourceName,
    tabsFor,
    type ItemTab,
  } from "./batchView";
  import { EMPTY_ACTIVITY, activityFromTimeline, applyActivityEvent, type ItemActivity } from "./itemActivity";
  import { applyLiveEvent, keepRunning, liveLineOf, type LiveEvent, type LiveStep } from "./liveSteps";
  import { createBatchView, shellIpc } from "./useBatch.svelte";

  interface Props {
    batchId: string;
    origin: BatchOrigin;
  }

  let { batchId, origin }: Props = $props();

  /** Pace of the readings no event asked for, while the batch may change. */
  const POLL_MS = 5_000;
  /** Pace of the clock the live steps are timed on. */
  const TICK_MS = 1_000;
  /** Categories of runtime events that mean an item moved. */
  const MOVING = new Set(["task-changed", "approval-changed"]);

  const view = createBatchView();

  let tab = $state<ItemTab>("all");
  let query = $state("");
  let now = $state(new Date());
  let confirmingStop = $state(false);
  let selectedRank = $state<number | null>(null);
  let activity = $state<ItemActivity>(EMPTY_ACTIVITY);
  let activityUnread = $state(false);
  let deciding = $state<BatchItem | null>(null);
  let accessLabels = $state<Record<string, string>>({});
  let liveSteps = $state<ReadonlyMap<string, LiveStep>>(new Map());

  $effect(() => {
    const asked = batchId;
    untrack(() => {
      tab = "all";
      query = "";
      closeItem();
      void view.load(asked);
    });
  });

  const batch = $derived(view.detail?.batch ?? null);
  const items = $derived(view.detail?.items ?? []);
  const live = $derived(batch !== null && isLive(batch));

  // The tasks of the items running now, whose steps are followed.
  const running = $derived(
    new Set(items.flatMap((item) => (item.status === "running" && item.taskId !== null ? [item.taskId] : []))),
  );

  $effect(() => {
    const still = running;
    untrack(() => {
      liveSteps = keepRunning(liveSteps, still);
    });
  });

  function follow(event: LiveEvent): void {
    const at = Date.now();
    liveSteps = applyLiveEvent(liveSteps, event, running, at);
    const task = selected?.taskId ?? null;
    if (task !== null) activity = applyActivityEvent(activity, event, task, at);
  }

  // The runtime says when a run or a decision moved; the batch is read again.
  // The steps of a run, and the tokens of its answer, say what the agent is
  // doing on each running item, as the conversation does.
  $effect(() => {
    const stops: UnlistenFn[] = [];
    let gone = false;
    const keep = (stop: UnlistenFn) => {
      if (gone) stop();
      else stops.push(stop);
    };
    void listen<LiveEvent & { category: string }>("runtime-event", (event) => {
      if (MOVING.has(event.payload.category)) void view.refresh();
      follow(event.payload);
    })
      .then(keep)
      .catch(() => {
        // No event bridge: the slow pace below still reads the batch.
      });
    void listen<Record<string, unknown>>("chat-token", (event) => {
      follow({ event_type: "ChatToken", payload: event.payload });
    })
      .then(keep)
      .catch(() => {
        // No token channel: the step stays the last one a run event named.
      });
    return () => {
      gone = true;
      for (const stop of stops) stop();
    };
  });

  // The clock of the live steps ticks every second; the batch is read again
  // at the slower pace.
  $effect(() => {
    if (!live) return;
    let ticks = 0;
    const timer = setInterval(() => {
      now = new Date();
      ticks += 1;
      if (ticks % (POLL_MS / TICK_MS) === 0) void view.refresh();
    }, TICK_MS);
    return () => clearInterval(timer);
  });

  // What each access granted lets the agent do, to name it in the summary.
  $effect(() => {
    const agent = batch?.agent;
    if (agent === undefined) return;
    let stale = false;
    listBatchAccesses(agent)
      .then((accesses) => {
        if (!stale) accessLabels = Object.fromEntries(accesses.map((access) => [access.accessId, access.permits]));
      })
      .catch(() => {
        // The accesses are then named by their identifier.
      });
    return () => {
      stale = true;
    };
  });

  const shownTab = $derived(shownItemTab(tab, items));
  const tabs = $derived(
    tabsFor(items).map((entry) => ({
      key: entry.key,
      label: $t(`batch_view.tab.${entry.key}`),
      count: entry.count,
    })),
  );
  const rows = $derived(filterItems(items, shownTab, query));

  const selected = $derived(
    selectedRank === null ? null : (items.find((item) => item.rank === selectedRank) ?? null),
  );

  function closeItem(): void {
    selectedRank = null;
    activity = EMPTY_ACTIVITY;
    activityUnread = false;
  }

  function selectItem(item: BatchItem): void {
    if (selectedRank === item.rank) {
      closeItem();
      return;
    }
    selectedRank = item.rank;
    activity = EMPTY_ACTIVITY;
    activityUnread = false;
    if (item.taskId !== null) loadActivity(item.rank, item.taskId);
  }

  // The course of the run as recorded, then what the run reported since the
  // item was opened, which the record may not hold yet.
  function loadActivity(rank: number, taskId: string): void {
    getTaskTimeline(taskId)
      .then((timeline) => {
        if (selectedRank !== rank) return;
        const recorded = activityFromTimeline(timeline);
        const last = recorded.entries.at(-1)?.at ?? 0;
        activity = {
          entries: [...recorded.entries, ...activity.entries.filter((entry) => entry.at > last)],
          writing: activity.writing,
        };
      })
      .catch(() => {
        if (selectedRank === rank) activityUnread = true;
      });
  }

  // An item opened before it ran is read from the record once its run starts.
  let followedTask: string | null = null;
  $effect(() => {
    const task = selected?.taskId ?? null;
    const rank = selected?.rank ?? null;
    untrack(() => {
      if (task !== null && rank !== null && task !== followedTask && activity.entries.length === 0) {
        loadActivity(rank, task);
      }
      followedTask = task;
    });
  });

  function act(action: BatchControl): void {
    if (action === "stop") {
      confirmingStop = true;
      return;
    }
    void view.act(action);
  }

  async function stop(): Promise<void> {
    await view.act("stop");
    confirmingStop = false;
  }

  async function exportAs(format: BatchExportFormat): Promise<void> {
    if (batch === null) return;
    let path: string | null;
    try {
      path = await saveDialog({
        defaultPath: exportFileName(batch, format),
        filters: [{ name: $t(`batch_view.export.${format}`), extensions: [format] }],
      });
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
      return;
    }
    if (typeof path !== "string") return;
    const written = await view.exportTo(format, path);
    if (written !== null) {
      addToast($t("batch_view.export.written", { values: { path: written } }), "success", {
        "data-testid": "batch-export-written",
      });
    }
  }

  const title = $derived.by(() => {
    if (batch === null) return $t("batch_view.title_loading");
    const date = new Date(parseTimestampMs(batch.createdAt));
    if (Number.isNaN(date.getTime())) return $t("batch_view.title_loading");
    const day = date.toLocaleDateString($locale ?? "en", { day: "numeric", month: "long" });
    const hour = date.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit" });
    return $t("batch_view.title", { values: { day, hour } });
  });

  const subtitle = $derived.by(() => {
    if (batch === null) return "";
    const values = { source: sourceName(batch.source.location), sheet: batch.source.options.sheet ?? "", count: batch.total };
    return $t(values.sheet === "" ? "batch_view.source" : "batch_view.source_sheet", { values });
  });
</script>

{#snippet headerActions()}
  {#if batch !== null}
    <BatchDeleteAction {batch} {items} ondeleted={openBatchHistory} />
  {/if}
  <Button variant="outline" size="sm" onclick={openBatchHistory} data-testid="batch-to-history">
    {$t(origin === "history" ? "batch_view.back_to_history" : "batch_view.history")}
  </Button>
{/snippet}

<PageLayout
  kicker={batch === null
    ? $t("batch_view.kicker_loading")
    : $t("batch_view.kicker", { values: { agent: batch.agent } })}
  {title}
  {subtitle}
  actions={headerActions}
  class="space-y-4 p-6"
  data-testid="batch-page"
>
  {#if view.error}
    <ErrorBanner
      tone="danger"
      message={$t("batch_view.load_failed")}
      onretry={() => void view.load(batchId)}
      loading={view.loading}
      data-testid="batch-load-error"
    />
  {:else if view.missing}
    <Card data-testid="batch-missing">
      <EmptyState tone="neutral" title={$t("batch_view.missing_title")} desc={$t("batch_view.missing_body")}>
        {#snippet icon()}
          <Layers size={22} strokeWidth={1.5} />
        {/snippet}
      </EmptyState>
    </Card>
  {:else if batch === null}
    <SkeletonList count={5} />
  {:else}
    <BatchSummaryCard {batch} busy={view.busy} {accessLabels} {now} onAct={act} onExport={(format) => void exportAs(format)} />

    {#if view.refusal}
      <ErrorBanner
        tone="warning"
        message={view.refusal}
        ondismiss={() => view.clearRefusal()}
        data-testid="batch-refusal"
      />
    {/if}

    <div class="flex flex-wrap items-center gap-3">
      <TabBar
        items={tabs}
        activeTab={shownTab}
        ontabchange={(key) => (tab = key as ItemTab)}
        testidPrefix="batch"
        variant="underline"
        class="max-w-full"
      />
      <Input
        bind:value={query}
        icon={Search}
        size="sm"
        class="ml-auto w-72"
        placeholder={$t("batch_view.search")}
        aria-label={$t("batch_view.search")}
        data-testid="batch-search"
      />
    </div>

    {#if rows.length === 0}
      <Card data-testid="batch-table-empty">
        <EmptyState
          tone="neutral"
          title={$t(items.length === 0 ? "batch_view.empty_title" : "batch_view.empty_filtered_title")}
          desc={$t(items.length === 0 ? "batch_view.empty_body" : "batch_view.empty_filtered_body")}
        >
          {#snippet icon()}
            <Layers size={22} strokeWidth={1.5} />
          {/snippet}
        </EmptyState>
      </Card>
    {:else}
      <div
        class="grid items-start gap-4 {selected === null ? '' : 'xl:grid-cols-[minmax(0,1fr)_minmax(26rem,44%)]'}"
        data-testid="batch-body"
      >
        <div class="min-w-0">
          <BatchItemsTable
            items={rows}
            {selectedRank}
            onSelect={selectItem}
            onOpenDecision={(item) => (deciding = item)}
            live={liveSteps}
            now={now.getTime()}
          />
        </div>
        {#if selected !== null}
          <div class="min-w-0 xl:sticky xl:top-4 xl:flex xl:max-h-[calc(100vh-7rem)] xl:flex-col">
            <BatchItemPanel
              item={selected}
              {activity}
              unread={activityUnread}
              liveLine={selected.status === "running"
                ? liveLineOf(selected, selected.taskId === null ? undefined : liveSteps.get(selected.taskId), now.getTime(), $t)
                : null}
              onclose={closeItem}
            />
          </div>
        {/if}
      </div>
    {/if}

    <ConfirmDialog
      open={confirmingStop}
      onclose={() => (confirmingStop = false)}
      onconfirm={() => void stop()}
      title={$t("batch_view.stop.title")}
      message={$t("batch_view.stop.body", {
        values: { treated: progressOf(batch).treated, remaining: progressOf(batch).remaining },
      })}
      confirmLabel={$t("batch_view.stop.confirm")}
      cancelLabel={$t("batch_view.stop.cancel")}
      loading={view.busy === "stop"}
      data-testid="batch-stop-dialog"
    />

    <BatchDecisionDialog
      {batchId}
      item={deciding}
      ipc={shellIpc}
      onclose={() => (deciding = null)}
      ondecided={() => void view.refresh()}
    />
  {/if}
</PageLayout>
