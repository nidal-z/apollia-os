<script lang="ts">
  /**
   * BatchScreen - the view of one batch: its summary and commands, then its
   * table, which fills as the items end. The batch is read again whenever the
   * runtime reports a run or a decision, and at a slow pace besides, so the
   * figures shown are always those the batch engine recorded.
   */
  import { onDestroy, untrack } from "svelte";
  import { locale, t } from "svelte-i18n";
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
  import { batchItemSelection, openBatchHistory, type BatchOrigin, type ItemStepsState } from "$lib/stores/batchView";
  import { detailSidebar } from "$lib/stores/detailSidebar";
  import BatchDecisionDialog from "./BatchDecisionDialog.svelte";
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
  import { createBatchView, shellIpc } from "./useBatch.svelte";

  interface Props {
    batchId: string;
    origin: BatchOrigin;
  }

  let { batchId, origin }: Props = $props();

  /** Pace of the readings no event asked for, while the batch may change. */
  const POLL_MS = 5_000;
  /** Categories of runtime events that mean an item moved. */
  const MOVING = new Set(["task-changed", "approval-changed"]);
  /** Steps of a run a user reads as steps. */
  const STEP_TYPES = new Set(["tool_call", "step_started"]);

  const view = createBatchView();

  let tab = $state<ItemTab>("all");
  let query = $state("");
  let now = $state(new Date());
  let confirmingStop = $state(false);
  let selectedRank = $state<number | null>(null);
  let steps = $state<ItemStepsState>({ status: "none" });
  let deciding = $state<BatchItem | null>(null);
  let accessLabels = $state<Record<string, string>>({});

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

  // The runtime says when a run or a decision moved; the batch is read again.
  $effect(() => {
    let unlisten: UnlistenFn | null = null;
    let gone = false;
    void listen<{ category: string }>("runtime-event", (event) => {
      if (MOVING.has(event.payload.category)) void view.refresh();
    })
      .then((stop) => {
        if (gone) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // No event bridge: the slow pace below still reads the batch.
      });
    return () => {
      gone = true;
      unlisten?.();
    };
  });

  $effect(() => {
    if (!live) return;
    const timer = setInterval(() => {
      now = new Date();
      void view.refresh();
    }, POLL_MS);
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

  // The detail sidebar shows the selected item as the batch reads it now.
  $effect(() => {
    if (selected === null || batch === null) {
      batchItemSelection.set(null);
      return;
    }
    batchItemSelection.set({ batch, item: selected, steps });
  });

  const sidebarOnItem = $derived($detailSidebar.open && $detailSidebar.tab === "item");

  function closeItem(): void {
    selectedRank = null;
    steps = { status: "none" };
    batchItemSelection.set(null);
    detailSidebar.closeItem();
  }

  onDestroy(closeItem);

  function selectItem(item: BatchItem): void {
    if (selectedRank === item.rank && sidebarOnItem) {
      closeItem();
      return;
    }
    selectedRank = item.rank;
    detailSidebar.openItem();
    if (item.taskId === null) {
      steps = { status: "none" };
      return;
    }
    const rank = item.rank;
    steps = { status: "loading" };
    getTaskTimeline(item.taskId)
      .then((timeline) => {
        if (selectedRank !== rank) return;
        steps = { status: "ready", steps: timeline.filter((step) => STEP_TYPES.has(step.type)) };
      })
      .catch(() => {
        if (selectedRank === rank) steps = { status: "error" };
      });
  }

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
    const date = new Date(batch.createdAt);
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
      <BatchItemsTable
        items={rows}
        selectedRank={sidebarOnItem ? selectedRank : null}
        onSelect={selectItem}
        onOpenDecision={(item) => (deciding = item)}
      />
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
