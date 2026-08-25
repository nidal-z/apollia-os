<script lang="ts">
  /**
   * Operator route `/automations` - table-style overview of every rule that
   * starts an assistant automatically (cron, interval, file-watch, webhook).
   *
   * The underlying backend entity is a "trigger". The operator vocabulary
   * surfaces it as "automation" / "run now" / "history" / "target assistant".
   */
  import { t, locale } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { triggers } from "$lib/stores/triggers";
  import type { TriggerStatus } from "$lib/types";
  import AutomationRow from "../components/automations/AutomationRow.svelte";
  import AutomationEmptyState from "../components/automations/AutomationEmptyState.svelte";
  import TriggerLogs from "../components/triggers/TriggerLogs.svelte";
  import CreateTriggerDialog from "../components/triggers/CreateTriggerDialog.svelte";
  import AutomationWizard from "../components/automations/AutomationWizard.svelte";
  import AutomationEditDialog from "../components/automations/AutomationEditDialog.svelte";
  import DeleteAutomationDialog from "../components/automations/DeleteAutomationDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { listTriggers, deleteTrigger } from "$lib/ipc/triggers";
  import {
    PageHeader,
    EmptyState,
    ErrorBanner,
    SkeletonList,
    FilterChipBar,
    ListPanel,
  } from "$lib/components/operator";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { Button } from "$lib/components/ui/button";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import { Plus, RefreshCw, Sparkles } from "lucide-svelte";

  type Filter = "all" | "active" | "paused" | "error";

  let logsTriggerId = $state<string | null>(null);
  let showWizard = $state(false);
  let showAdvancedDialog = $state(false);
  let editTriggerId = $state<string | null>(null);
  let deleteCandidate = $state<{ id: string; fireCount: number } | null>(null);
  let deleting = $state(false);
  let activeFilter = $state<Filter>("all");
  let refreshing = $state(false);
  let loadError = $state<HumanizedError | null>(null);
  // Snippet closures (ErrorBanner / Disclosure children) drop `{#if loadError}`
  // narrowing, so expose the fields as always-defined deriveds.
  const errorTitle = $derived(loadError?.title ?? "");
  const errorAction = $derived(loadError?.suggested_action ?? "");
  const errorDetail = $derived(loadError?.detail ?? "");

  const DELETE_SKIP_KEY = "apollia.delete_automation.skip";

  function statusOf(t: TriggerStatus): "active" | "paused" | "error" {
    return !t.enabled ? "paused" : "active";
  }

  const counts = $derived.by(() => {
    const c: Record<Filter, number> = { all: $triggers.length, active: 0, paused: 0, error: 0 };
    for (const trig of $triggers) {
      c[statusOf(trig)] = (c[statusOf(trig)] ?? 0) + 1;
    }
    return c;
  });

  const filteredTriggers = $derived(
    activeFilter === "all"
      ? $triggers
      : $triggers.filter((t) => statusOf(t) === activeFilter),
  );

  const FILTERS: { key: Filter; labelKey: string; tone: "primary" | "success" | "neutral" | "danger"; color: string }[] = [
    { key: "all",    labelKey: "automations.filters.all",    tone: "primary", color: "hsl(var(--muted-foreground))" },
    { key: "active", labelKey: "automations.filters.active", tone: "success", color: "hsl(var(--success))" },
    { key: "paused", labelKey: "automations.filters.paused", tone: "neutral", color: "hsl(var(--muted-foreground))" },
    { key: "error",  labelKey: "automations.filters.error",  tone: "danger", color: "hsl(var(--destructive))" },
  ];

  function handleFired(taskId: string) {
    addToast(
      $t("automations.fired_toast", { values: { taskId: taskId.slice(0, 8) } }),
      "success",
    );
  }

  function handleOpenHistory(triggerId: string) {
    logsTriggerId = triggerId;
  }

  function handleCloseHistory() {
    logsTriggerId = null;
  }

  function handleCreate() {
    showWizard = true;
  }

  function handleRequestEdit(triggerId: string) {
    editTriggerId = triggerId;
  }

  // Same confirmation pattern as the other write actions of this page: success
  // toast, then a reload so the row shows the schedule that is now stored.
  async function handleEdited(triggerId: string) {
    editTriggerId = null;
    addToast(
      $t("triggers.updated_toast", { values: { id: triggerId } }),
      "success",
    );
    await handleRefresh();
  }

  function handleSwitchAdvanced() {
    showWizard = false;
    showAdvancedDialog = true;
  }

  async function handleRefresh() {
    refreshing = true;
    loadError = null;
    try {
      // Call the IPC directly (not the store's `refreshTriggers`, which
      // swallows errors) so a failure surfaces in the humanized banner; the
      // raw text stays available under the Disclosure below.
      const result = await listTriggers();
      triggers.set(result);
    } catch (err) {
      loadError = reportError(err, { surface: "inline" });
    } finally {
      refreshing = false;
    }
  }

  $effect(() => {
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<{ triggerId: string }>).detail;
      if (detail?.triggerId) handleRequestDelete(detail.triggerId);
    };
    window.addEventListener("apollia:automation_delete_request", handler);
    const createHandler = () => {
      showWizard = true;
    };
    window.addEventListener("apollia:automations:create", createHandler);
    return () => {
      window.removeEventListener("apollia:automation_delete_request", handler);
      window.removeEventListener("apollia:automations:create", createHandler);
    };
  });

  function handleRequestDelete(triggerId: string) {
    const trig = $triggers.find((t) => t.id === triggerId);
    const fireCount = trig?.fire_count ?? 0;
    if (localStorage.getItem(DELETE_SKIP_KEY) === "1") {
      void performDelete(triggerId);
      return;
    }
    deleteCandidate = { id: triggerId, fireCount };
  }

  async function performDelete(triggerId: string) {
    deleting = true;
    try {
      await deleteTrigger(triggerId);
      addToast(
        $t("triggers.deleted_toast", { values: { id: triggerId } }),
        "success",
      );
    } catch (err) {
      addToast(
        $t("triggers.delete_error", {
          values: { message: err instanceof Error ? err.message : String(err) },
        }),
        "error",
      );
    } finally {
      deleting = false;
      deleteCandidate = null;
    }
  }

  async function handleConfirmDelete(skipNext: boolean) {
    if (!deleteCandidate) return;
    if (skipNext) {
      localStorage.setItem(DELETE_SKIP_KEY, "1");
    }
    await performDelete(deleteCandidate.id);
  }
</script>

<div class="mx-auto w-full max-w-6xl" data-testid="automations-page">
  <PageHeader
    title={$t("automations.title")}
    subtitle={$t("automations.subtitle")}
  >
    {#snippet actions()}
      <Button
        variant="outline"
        size="sm"
        onclick={handleRefresh}
        disabled={refreshing}
        data-testid="automations-refresh-btn"
      >
        {#snippet icon()}<RefreshCw size={12} class={refreshing ? "animate-spin" : ""} />{/snippet}
        {$t("common.refresh")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={handleCreate} data-testid="automations-new-btn">
        {#snippet icon()}<Plus size={12} />{/snippet}
        {$t("automations.new")}
      </Button>
    {/snippet}
  </PageHeader>

  {#if loadError}
    <!-- Humanized load failure: action-first copy, raw text behind Disclosure -->
    <div class="px-8 pt-4 space-y-2" data-testid="automations-load-error">
      <ErrorBanner
        tone="danger"
        onretry={handleRefresh}
        retryLabel={$t("common.retry")}
        loading={refreshing}
      >
        <p class="font-medium text-foreground">{errorTitle}</p>
        <p class="text-muted-foreground">{errorAction}</p>
      </ErrorBanner>
      {#if errorDetail}
        <Disclosure open={false} testid="automations-load-error-details">
          {#snippet summary()}
            <span class="font-medium text-foreground">{$t("automations.technical_details")}</span>
          {/snippet}
          {#snippet children()}
            <pre class="overflow-x-auto whitespace-pre-wrap break-words font-mono text-caption text-muted-foreground">{errorDetail}</pre>
          {/snippet}
        </Disclosure>
      {/if}
    </div>
  {/if}

  {#if refreshing && $triggers.length === 0}
    <!-- First-load skeleton: same column rhythm to avoid layout shift -->
    <div class="px-8 pt-6" data-testid="automations-loading">
      <ListPanel>
        <div class="px-4 py-3">
          <SkeletonList count={4} rowClass="py-1" />
        </div>
      </ListPanel>
    </div>
  {:else if $triggers.length === 0}
    <div class="px-8 pt-6">
      <AutomationEmptyState onCreate={handleCreate} />
    </div>
  {:else}
    <!-- Status filter chip row -->
    <FilterChipBar
      chips={FILTERS.map((f) => ({
        key: f.key,
        label: $t(f.labelKey),
        color: f.color,
        count: counts[f.key],
        glowWhenActive: f.key === "active",
      }))}
      activeKey={activeFilter}
      onchange={(key) => (activeFilter = key as Filter)}
      aria-label={$t("automations.status_filters_aria")}
      testidPrefix="automations-filter"
      data-testid="automations-filter-bar"
    />

    <!-- Automations table -->
    <div class="px-8 pb-10">
      {#if filteredTriggers.length === 0}
        <ListPanel data-testid="automations-empty-filter">
          <EmptyState
            title={activeFilter === "all"
              ? $t("automations.empty.title")
              : $t("automations.empty_filter_title")}
            desc={activeFilter === "all"
              ? $t("automations.empty.description")
              : $t("automations.empty_filter_desc")}
          >
            {#snippet icon()}<Sparkles size={22} />{/snippet}
            {#snippet action()}
              {#if activeFilter !== "all"}
                <Button variant="outline" size="sm" onclick={() => (activeFilter = "all")}>
                  {$t("automations.filter_show_all")}
                </Button>
              {/if}
            {/snippet}
          </EmptyState>
        </ListPanel>
      {:else}
        <!-- Focal panel: brand-tinted depth marks this as the primary surface.
             Keyboard nav (roving tabindex) via `listNavigation`. -->
        <div use:listNavigation={{ rowSelector: "[data-automation-nav]" }}>
          <ListPanel
            data-testid="automations-table"
            class="shadow-primary-lg border-primary/20"
            columns={[
              { label: $t("automations.col_automation"), class: "flex-[2] min-w-0" },
              { label: $t("automations.col_assistant"), class: "w-40" },
              { label: $t("automations.col_next_run"), class: "w-40" },
              { label: $t("automations.col_status"), class: "w-28" },
              { label: $t("automations.col_last_run"), class: "w-24 text-right" },
              { label: "", class: "w-16" },
            ]}
          >
            {#each filteredTriggers as trigger (trigger.id)}
              <div animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
                <AutomationRow
                  {trigger}
                  locale={$locale ?? "en"}
                  onfire={handleFired}
                  onlogs={handleOpenHistory}
                  onedit={handleRequestEdit}
                  ondelete={handleRequestDelete}
                />
              </div>
            {/each}
          </ListPanel>
        </div>
      {/if}
    </div>
  {/if}
</div>

{#if logsTriggerId}
  <TriggerLogs
    triggerId={logsTriggerId}
    open={logsTriggerId !== null}
    onclose={handleCloseHistory}
  />
{/if}

<AutomationWizard
  open={showWizard}
  onclose={() => (showWizard = false)}
  oncreated={(id) => {
    showWizard = false;
    addToast($t("triggers.created_toast", { values: { id } }), "success");
  }}
  onswitchadvanced={handleSwitchAdvanced}
/>

<CreateTriggerDialog
  open={showAdvancedDialog}
  onclose={() => (showAdvancedDialog = false)}
  oncreated={() => (showAdvancedDialog = false)}
/>

{#if editTriggerId}
  <AutomationEditDialog
    open={true}
    triggerId={editTriggerId}
    onclose={() => (editTriggerId = null)}
    onsaved={(id) => void handleEdited(id)}
  />
{/if}

{#if deleteCandidate}
  <DeleteAutomationDialog
    open={true}
    automationId={deleteCandidate.id}
    fireCount={deleteCandidate.fireCount}
    loading={deleting}
    onclose={() => (deleteCandidate = null)}
    onconfirm={handleConfirmDelete}
  />
{/if}
