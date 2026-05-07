<script lang="ts">
  /**
   * Operator route `/automations` — table-style overview of every rule that
   * starts an assistant automatically (cron, interval, file-watch, webhook).
   *
   * The underlying backend entity is a "trigger". The operator vocabulary
   * surfaces it as "automation" / "run now" / "history" / "target assistant".
   */
  import { t, locale } from "svelte-i18n";
  import { triggers } from "$lib/stores/triggers";
  import { currentRoute } from "$lib/stores/navigation";
  import type { TriggerStatus } from "$lib/types";
  import AutomationRow from "../components/automations/AutomationRow.svelte";
  import AutomationEmptyState from "../components/automations/AutomationEmptyState.svelte";
  import TriggerLogs from "../components/triggers/TriggerLogs.svelte";
  import CreateTriggerDialog from "../components/triggers/CreateTriggerDialog.svelte";
  import AutomationWizard from "../components/automations/AutomationWizard.svelte";
  import DeleteAutomationDialog from "../components/automations/DeleteAutomationDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { invoke } from "@tauri-apps/api/core";
  import {
    PageHeader,
    Chip,
    StatusDot,
    BtnPrimary,
    BtnSecondary,
    EmptyState,
  } from "$lib/components/operator";
  import { Plus, RefreshCw, Sparkles } from "lucide-svelte";

  type Filter = "all" | "active" | "paused" | "error";

  let logsTriggerId = $state<string | null>(null);
  let showWizard = $state(false);
  let showAdvancedDialog = $state(false);
  let deleteCandidate = $state<{ id: string; fireCount: number } | null>(null);
  let deleting = $state(false);
  let activeFilter = $state<Filter>("all");
  let refreshing = $state(false);

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

  const FILTERS: { key: Filter; label: string; tone: "primary" | "success" | "neutral" | "danger"; color: string }[] = [
    { key: "all",    label: "Toutes", tone: "primary", color: "hsl(var(--muted-foreground))" },
    { key: "active", label: "Actives", tone: "success", color: "hsl(var(--success))" },
    { key: "paused", label: "En pause", tone: "neutral", color: "hsl(var(--muted-foreground))" },
    { key: "error",  label: "Erreur",  tone: "danger", color: "hsl(var(--destructive))" },
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

  function handleBrowseTemplates() {
    if (typeof window !== "undefined") {
      const url = new URL(window.location.href);
      url.search = "?kind=automation";
      window.history.replaceState({}, "", url.toString());
    }
    currentRoute.set("templates");
  }

  function handleSwitchAdvanced() {
    showWizard = false;
    showAdvancedDialog = true;
  }

  async function handleRefresh() {
    refreshing = true;
    try {
      // Trigger store auto-refreshes via SSE — manual refresh is a no-op
      // hint to the user. Add real reload call here if/when exposed.
      await new Promise((r) => setTimeout(r, 250));
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
      await invoke("delete_trigger", { id: triggerId });
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
    kicker="AUTOMATISATIONS"
    title={$t("automations.title")}
    subtitle={$t("automations.subtitle")}
  >
    {#snippet actions()}
      <BtnSecondary onclick={handleRefresh} disabled={refreshing}>
        {#snippet icon()}<RefreshCw size={12} class={refreshing ? "animate-spin" : ""} />{/snippet}
        {$t("common.refresh")}
      </BtnSecondary>
      <BtnPrimary onclick={handleCreate}>
        {#snippet icon()}<Plus size={12} />{/snippet}
        {$t("automations.new")}
      </BtnPrimary>
    {/snippet}
  </PageHeader>

  {#if $triggers.length === 0}
    <div class="px-8 pt-6">
      <AutomationEmptyState onCreate={handleCreate} onBrowseTemplates={handleBrowseTemplates} />
    </div>
  {:else}
    <!-- Status filter chip row -->
    <div
      class="flex flex-wrap items-center gap-2 px-8 pt-5 pb-4"
      role="tablist"
      aria-label="Filtres de statut"
      data-testid="automations-filter-bar"
    >
      {#each FILTERS as f (f.key)}
        {@const isActive = activeFilter === f.key}
        <button
          type="button"
          role="tab"
          aria-selected={isActive}
          onclick={() => (activeFilter = f.key)}
          class="cursor-pointer border-0 bg-transparent p-0"
          data-testid="automations-filter-{f.key}"
          data-active={isActive}
        >
          <Chip
            tone={isActive ? f.tone : "neutral"}
            outline={!isActive}
            size="md"
          >
            {#snippet icon()}
              <StatusDot color={f.color} glow={isActive && f.key === "active"} />
            {/snippet}
            {f.label} · {counts[f.key]}
          </Chip>
        </button>
      {/each}
    </div>

    <!-- Automations table -->
    <div class="px-8 pb-10">
      {#if filteredTriggers.length === 0}
        <div
          class="rounded-xl border border-border/60 bg-card"
          data-testid="automations-empty-filter"
        >
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
                <BtnSecondary onclick={() => (activeFilter = "all")}>
                  {$t("automations.filter_show_all")}
                </BtnSecondary>
              {/if}
            {/snippet}
          </EmptyState>
        </div>
      {:else}
        <div
          class="rounded-xl border border-border/60 bg-card"
          data-testid="automations-table"
        >
          <!-- Column headers -->
          <div
            class="px-4 py-2.5 border-b border-border/60 flex items-center gap-2.5 text-[10.5px] uppercase tracking-[1px] font-semibold text-muted-foreground/70"
          >
            <div class="flex-[2] min-w-0">{$t("automations.col_automation")}</div>
            <div class="w-[160px]">{$t("automations.col_assistant")}</div>
            <div class="w-[160px]">{$t("automations.col_next_run")}</div>
            <div class="w-[110px]">{$t("automations.col_status")}</div>
            <div class="w-[90px] text-right">{$t("automations.col_last_run")}</div>
            <div class="w-[64px]"></div>
          </div>

          {#each filteredTriggers as trigger (trigger.id)}
            <AutomationRow
              {trigger}
              locale={$locale ?? "en"}
              onfire={handleFired}
              onlogs={handleOpenHistory}
              ondelete={handleRequestDelete}
            />
          {/each}
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
