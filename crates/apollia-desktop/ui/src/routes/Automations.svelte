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
  import { uiMode } from "$lib/stores/mode";
  import type { TriggerStatus } from "$lib/types";
  import AutomationCard from "../components/automations/AutomationCard.svelte";
  import AutomationGroup from "../components/automations/AutomationGroup.svelte";
  import AutomationEmptyState from "../components/automations/AutomationEmptyState.svelte";
  import TriggerLogs from "../components/triggers/TriggerLogs.svelte";
  import CreateTriggerDialog from "../components/triggers/CreateTriggerDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";

  let logsTriggerId = $state<string | null>(null);
  let showCreateDialog = $state(false);

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
    showCreateDialog = true;
  }

  function handleBrowseTemplates() {
    // US-SP42-058 ships the template gallery — until then we surface a
    // breadcrumb via the toast so the operator knows the CTA is live.
    addToast($t("automations.templates_coming_soon"), "info");
  }

  function switchToBuilder() {
    uiMode.set("builder");
    currentRoute.set("triggers");
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

  <!-- Builder escape hatch — mirrors the pattern used on Agents / Projects. -->
  <footer class="flex justify-end pt-2">
    <button
      class="text-[11px] text-muted-foreground/70 underline decoration-dotted underline-offset-2 hover:text-muted-foreground"
      onclick={switchToBuilder}
      data-testid="automations-advanced-link"
    >
      {$t("automations.advanced_link")}
    </button>
  </footer>
</div>

{#if logsTriggerId}
  <TriggerLogs
    triggerId={logsTriggerId}
    open={logsTriggerId !== null}
    onclose={handleCloseHistory}
  />
{/if}

<CreateTriggerDialog
  open={showCreateDialog}
  onclose={() => (showCreateDialog = false)}
  oncreated={() => (showCreateDialog = false)}
/>
