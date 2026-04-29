<script lang="ts">
  /**
   * Operator route `/automations` — card-based overview of every rule that
   * starts an assistant automatically (cron, interval, file-watch, webhook).
   *
   * The underlying backend entity is still a "trigger" — here we surface it
   * in operator vocabulary: "automation", "run now", "history", "target
   * assistant". The builder view (`/triggers`) keeps the raw terminology.
   */
  import { t, locale } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { triggers } from "$lib/stores/triggers";
  import { currentRoute } from "$lib/stores/navigation";
  import type { TriggerStatus } from "$lib/types";
  import AutomationCard from "../components/automations/AutomationCard.svelte";
  import AutomationGroup from "../components/automations/AutomationGroup.svelte";
  import AutomationEmptyState from "../components/automations/AutomationEmptyState.svelte";
  import TriggerLogs from "../components/triggers/TriggerLogs.svelte";
  import CreateTriggerDialog from "../components/triggers/CreateTriggerDialog.svelte";
  import AutomationWizard from "../components/automations/AutomationWizard.svelte";
  import DeleteAutomationDialog from "../components/automations/DeleteAutomationDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { invoke } from "@tauri-apps/api/core";

  let logsTriggerId = $state<string | null>(null);
  let showWizard = $state(false);
  let showAdvancedDialog = $state(false);
  let deleteCandidate = $state<{ id: string; fireCount: number } | null>(null);
  let deleting = $state(false);
  const DELETE_SKIP_KEY = "apollia.delete_automation.skip";

  /** Group triggers by agent, remembering insertion order. */
  const grouped = $derived.by(() => {
    const groups = new Map<string, TriggerStatus[]>();
    for (const trigger of $triggers) {
      const existing = groups.get(trigger.agent);
      if (existing) {
        existing.push(trigger);
      } else {
        groups.set(trigger.agent, [trigger]);
      }
    }
    return [...groups.entries()];
  });

  /** Default-open group = assistant with the most-recent `last_fired`. */
  const defaultOpenAgent = $derived.by(() => {
    let bestAgent: string | null = null;
    let bestTime = -Infinity;
    for (const [agent, list] of grouped) {
      for (const trig of list) {
        if (trig.last_fired) {
          const t = new Date(trig.last_fired).getTime();
          if (t > bestTime) {
            bestTime = t;
            bestAgent = agent;
          }
        }
      }
    }
    // Fall back to the first group when no runs have been recorded yet.
    if (!bestAgent && grouped.length > 0) return grouped[0][0];
    return bestAgent;
  });

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
    // deep-link to the gallery scoped to automations.
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

  $effect(() => {
    // Listen for delete requests emitted by automation cards / context menus.
    // Decoupled via `window` event so cards (and future UI like context menus)
    // don't need to know about the route-level dialog state.
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
      await invoke("delete_trigger", { triggerId });
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

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="automations-page">
  <header class="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
    <div>
      <h1 class="text-2xl font-semibold" data-testid="automations-title">
        {$t("automations.title")}
      </h1>
      <p class="mt-0.5 text-sm text-muted-foreground">{$t("automations.subtitle")}</p>
    </div>
    {#if $triggers.length > 0}
      <button
        class="text-xs text-muted-foreground underline decoration-dotted underline-offset-2 hover:text-foreground"
        onclick={handleCreate}
        data-testid="automations-create-cta"
      >
        {$t("automations.new")}
      </button>
    {/if}
  </header>

  {#if $triggers.length === 0}
    <AutomationEmptyState onCreate={handleCreate} onBrowseTemplates={handleBrowseTemplates} />
  {:else}
    <div class="space-y-4" data-testid="automations-list">
      {#each grouped as [agentName, list] (agentName)}
        <div animate:flip={{ duration: 250 }} in:fly={{ y: 8, duration: 180 }}>
          <AutomationGroup
            {agentName}
            count={list.length}
            defaultOpen={agentName === defaultOpenAgent}
          >
            {#each list as trigger (trigger.id)}
              <AutomationCard
                {trigger}
                locale={$locale ?? "en"}
                onfire={handleFired}
                onlogs={handleOpenHistory}
              />
            {/each}
          </AutomationGroup>
        </div>
      {/each}
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
