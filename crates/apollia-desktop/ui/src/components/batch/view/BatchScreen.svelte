<script lang="ts">
  /**
   * BatchScreen - the view of one batch: a bar that stays on top with its
   * progress, pace and commands; below, the list of its items beside the
   * detail of one, the item running by default, followed as it changes. The
   * batch is read again whenever the runtime reports a run or a decision, and
   * at a slow pace besides, so the figures shown are those the engine recorded.
   */
  import { untrack } from "svelte";
  import { locale, t } from "svelte-i18n";
  import { parseTimestampMs } from "$lib/utils";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { Layers, MessageSquare, Search } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { Input } from "$lib/components/ui/input";
  import { TabBar } from "$lib/components/ui/tabs";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Card, EmptyState, ErrorBanner, PageLayout, SkeletonList } from "$lib/components/operator";
  import { listBatchAccesses, listBatches, type BatchSummary } from "$lib/ipc/batches";
  import type { BatchControl, BatchExportFormat, BatchItem } from "$lib/ipc/batches";
  import { getTaskTimeline } from "$lib/ipc/tasks";
  import { openBatchHistory, type BatchOrigin } from "$lib/stores/batchView";
  import BatchDecisionDialog from "./BatchDecisionDialog.svelte";
  import BatchDeleteAction from "./BatchDeleteAction.svelte";
  import BatchItemPanel from "./BatchItemPanel.svelte";
  import BatchItemList from "./BatchItemList.svelte";
  import BatchSummaryCard from "./BatchSummaryCard.svelte";
  import BatchSynthesisCard from "./BatchSynthesisCard.svelte";
  import BatchRequestCard from "./BatchRequestCard.svelte";
  import BatchResults from "./BatchResults.svelte";
  import { outputsOf, resultRows, resultsCsv } from "./batchResults";
  import { createChatSession } from "$lib/ipc/agents";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId, pendingComposer } from "$lib/stores/chat";
  import { reportError } from "$lib/errors/reportError";
  import {
    exportFileName,
    filterItems,
    isLive,
    isMoving,
    synthesisOf,
    itemPace,
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
  /** The item chosen by hand, `null` while the running one is followed. */
  let chosenRank = $state<number | null>(null);
  /** The detail follows the item running, until one is chosen by hand. */
  let followRunning = $state(true);
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
      chosenRank = null;
      followRunning = true;
      void view.load(asked);
    });
  });

  const batch = $derived(view.detail?.batch ?? null);
  const items = $derived(view.detail?.items ?? []);
  const live = $derived(batch !== null && isLive(batch));
  // A completed batch is still read while the synthesis it asked for is written.
  const moving = $derived(batch !== null && isMoving(batch));
  const synthesis = $derived(batch === null ? null : synthesisOf(batch));

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
    if (!moving) return;
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

  // The item running now, said in the summary as the table says it.
  const current = $derived.by(() => {
    const item = items.find((entry) => entry.status === "running");
    if (item === undefined) return null;
    const step = item.taskId === null ? undefined : liveSteps.get(item.taskId);
    return { number: item.rank + 1, line: liveLineOf(item, step, now.getTime(), $t) };
  });

  // One batch runs at a time: a running batch with nothing running waits for
  // the batch launched before it, which the summary names.
  let others = $state<BatchSummary[]>([]);
  const waitingFor = $derived.by(() => {
    if (batch === null || batch.state !== "running" || current !== null) return null;
    const ahead = others.find(
      (other) => other.batchId !== batch.batchId && (other.state === "running" || other.state === "paused") && other.createdAt < batch.createdAt,
    );
    return ahead === undefined ? null : { agent: ahead.agent, started: ahead.createdAt };
  });
  $effect(() => {
    // Read again with the batch, at its own pace, not with the clock.
    if (batch === null || batch.state !== "running") return;
    let stale = false;
    listBatches(20)
      .then((list) => {
        if (!stale) others = list;
      })
      .catch(() => {
        // The waiting notice is left out.
      });
    return () => {
      stale = true;
    };
  });

  // The item in the detail: the one running while it is followed, else the
  // one chosen, else the first, so the detail is never empty.
  const runningItem = $derived(items.find((item) => item.status === "running") ?? null);
  const selected = $derived.by(() => {
    if (followRunning && runningItem !== null) return runningItem;
    const chosen = chosenRank === null ? null : (items.find((item) => item.rank === chosenRank) ?? null);
    return chosen ?? runningItem ?? items[0] ?? null;
  });

  function selectItem(item: BatchItem): void {
    chosenRank = item.rank;
    followRunning = false;
  }

  // The results read as the launch asked for them, once there are some; the
  // items one by one otherwise, and while the batch runs.
  let surfaceChosen = $state<"results" | "items" | null>(null);
  /** The files exported from this view, opened from the card of the request. */
  let exported = $state<string[]>([]);
  const outputs = $derived(batch === null ? [] : outputsOf(batch));
  const resultsOffered = $derived(outputs.length > 0 && items.some((item) => item.status === "completed"));
  const surface = $derived(resultsOffered ? (surfaceChosen ?? (live ? "items" : "results")) : "items");
  const producedFiles = $derived.by(() => {
    const files: { kind: "results" | "synthesis" | "export"; path: string }[] = [];
    const results = batch?.source.results?.path;
    if (results) files.push({ kind: "results", path: results });
    const written = batch?.source.synthesis?.path;
    if (written) files.push({ kind: "synthesis", path: written });
    for (const path of exported) if (!files.some((file) => file.path === path)) files.push({ kind: "export", path });
    return files;
  });

  let continuing = $state(false);

  /**
   * A conversation with the agent of the batch, its results joined as a
   * table and the start of a request written in the composer: the person
   * says what to do with the forty answers, and sends.
   */
  async function continueInChat(): Promise<void> {
    if (batch === null || continuing) return;
    continuing = true;
    try {
      const source = sourceName(batch.source.location);
      const csv = resultsCsv(
        resultRows(items, outputs),
        outputs,
        $t("batch_view.results.item"),
        $t("batch_view.continue.status"),
      );
      const file = new File([csv], $t("batch_view.continue.file_name", { values: { source } }), { type: "text/csv" });
      const session = await createChatSession({ mode: "agent", agent_name: batch.agent });
      pendingComposer.set({
        sessionId: session.id,
        text: $t("batch_view.continue.prompt", { values: { source, count: progressOf(batch).treated } }),
        files: [file],
      });
      pendingChatSessionId.set(session.id);
      navigateTo("chat");
    } catch (err) {
      reportError(err, { surface: "toast", testid: "batch-continue-error" });
    } finally {
      continuing = false;
    }
  }

  function openFromResults(rank: number): void {
    const item = items.find((candidate) => candidate.rank === rank);
    if (item === undefined) return;
    selectItem(item);
    tab = "all";
    query = "";
    surfaceChosen = "items";
  }

  // The course of the run as recorded, then what the run reported since the
  // item was opened, which the record may not hold yet.
  function loadActivity(rank: number, taskId: string): void {
    getTaskTimeline(taskId)
      .then((timeline) => {
        if (selected?.rank !== rank) return;
        const recorded = activityFromTimeline(timeline);
        const last = recorded.entries.at(-1)?.at ?? 0;
        activity = {
          entries: [...recorded.entries, ...activity.entries.filter((entry) => entry.at > last)],
          writing: activity.writing,
        };
      })
      .catch(() => {
        if (selected?.rank === rank) activityUnread = true;
      });
  }

  // A new item in the detail, or the same item whose run starts, is read from
  // the record; the events of its run follow.
  let shownKey = "";
  $effect(() => {
    const key = selected === null ? "" : `${selected.rank}:${selected.taskId ?? ""}`;
    const rank = selected?.rank ?? null;
    const task = selected?.taskId ?? null;
    untrack(() => {
      if (key === shownKey) return;
      shownKey = key;
      activity = EMPTY_ACTIVITY;
      activityUnread = false;
      if (rank !== null && task !== null) loadActivity(rank, task);
    });
  });

  const perItemMs = $derived(itemPace(items));

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
      exported = [written, ...exported.filter((path) => path !== written)];
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
  maxWidth="max-w-none"
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
    <BatchSummaryCard
      {batch}
      busy={view.busy}
      {accessLabels}
      {now}
      onAct={act}
      onExport={(format) => void exportAs(format)}
      {current}
      {waitingFor}
      {perItemMs}
    />

    {#if batch.source.task !== undefined || producedFiles.length > 0}
      <BatchRequestCard task={batch.source.task ?? null} files={producedFiles} />
    {/if}

    {#if synthesis !== null}
      <BatchSynthesisCard {synthesis} />
    {/if}

    {#if view.refusal}
      <ErrorBanner
        tone="warning"
        message={view.refusal}
        ondismiss={() => view.clearRefusal()}
        data-testid="batch-refusal"
      />
    {/if}

    {#if resultsOffered}
      <div class="flex flex-wrap items-center justify-between gap-3">
      <TabBar
        items={[
          { key: "results", label: $t("batch_view.surface.results") },
          { key: "items", label: $t("batch_view.surface.items"), count: items.length },
        ]}
        activeTab={surface}
        ontabchange={(key) => (surfaceChosen = key as "results" | "items")}
        testidPrefix="batch-surface"
        variant="pill"
      />
      {#if !live}
        <Button variant="primary-solid" size="sm" onclick={() => void continueInChat()} loading={continuing} data-testid="batch-continue-chat">
          {#snippet icon()}<MessageSquare size={13} />{/snippet}
          {$t("batch_view.continue.button")}
        </Button>
      {/if}
      </div>
    {/if}

    {#if surface === "results"}
      <BatchResults {items} {outputs} onopen={openFromResults} />
    {:else}
    <div class="grid items-start gap-4 lg:grid-cols-[minmax(18rem,26rem)_minmax(0,1fr)]" data-testid="batch-body">
      <Card class="flex min-w-0 flex-col overflow-hidden p-0 lg:sticky lg:top-28 lg:max-h-[calc(100vh-9rem)]">
        <div class="shrink-0 space-y-2 border-b border-border p-3">
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
            class="w-full"
            placeholder={$t("batch_view.search")}
            aria-label={$t("batch_view.search")}
            data-testid="batch-search"
          />
          {#if live}
            <label class="flex items-center gap-2 text-caption text-muted-foreground">
              <input
                type="checkbox"
                class="accent-primary"
                bind:checked={followRunning}
                data-testid="batch-follow-running"
              />
              {$t("batch_view.follow_running")}
            </label>
          {/if}
        </div>
        <div class="min-h-0 flex-1 overflow-y-auto">
          {#if rows.length === 0}
            <div class="p-4" data-testid="batch-table-empty">
              <EmptyState
                tone="neutral"
                title={$t(items.length === 0 ? "batch_view.empty_title" : "batch_view.empty_filtered_title")}
                desc={$t(items.length === 0 ? "batch_view.empty_body" : "batch_view.empty_filtered_body")}
              >
                {#snippet icon()}
                  <Layers size={22} strokeWidth={1.5} />
                {/snippet}
              </EmptyState>
            </div>
          {:else}
            <BatchItemList
              items={rows}
              selectedRank={selected?.rank ?? null}
              onSelect={selectItem}
              onOpenDecision={(item) => (deciding = item)}
              live={liveSteps}
              now={now.getTime()}
            />
          {/if}
        </div>
      </Card>

      {#if selected !== null}
        <div class="min-w-0 lg:sticky lg:top-28 lg:flex lg:max-h-[calc(100vh-9rem)] lg:flex-col">
          <BatchItemPanel
            item={selected}
            {activity}
            unread={activityUnread}
            liveLine={selected.status === "running"
              ? liveLineOf(selected, selected.taskId === null ? undefined : liveSteps.get(selected.taskId), now.getTime(), $t)
              : null}
            now={now.getTime()}
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
