<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t, locale } from "svelte-i18n";
  import { triggers } from "$lib/stores/triggers";
  import { uiMode } from "$lib/stores/mode";
  import { currentRoute } from "$lib/stores/navigation";
  import type { TriggerReloadResult, TriggerStatus } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import TriggerRow from "../components/triggers/TriggerRow.svelte";
  import TriggerLogs from "../components/triggers/TriggerLogs.svelte";
  import CreateTriggerDialog from "../components/triggers/CreateTriggerDialog.svelte";
  import EditTriggerDialog from "../components/triggers/EditTriggerDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { EmptyState } from "$lib/components/layout";
  import { EMPTY_STATES } from "$lib/i18n/strings/empty-states";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";

  let reloading = $state(false);
  let reloadError = $state<string | null>(null);

  let logsTriggerId = $state<string | null>(null);
  let logsOpen = $derived(logsTriggerId !== null);

  let showCreateDialog = $state(false);
  let editTriggerId = $state<string | null>(null);
  let deleteTriggerId = $state<string | null>(null);
  let deleting = $state(false);

  /** Group triggers by agent name, preserving insertion order. */
  const triggersByAgent = $derived.by(() => {
    const groups = new Map<string, TriggerStatus[]>();
    for (const trigger of $triggers) {
      const existing = groups.get(trigger.agent);
      if (existing) {
        existing.push(trigger);
      } else {
        groups.set(trigger.agent, [trigger]);
      }
    }
    return groups;
  });

  async function handleReload() {
    reloading = true;
    reloadError = null;
    try {
      const result: TriggerReloadResult = await invoke("reload_triggers");
      addToast($t('triggers.reload_success', { values: { count: result.reloaded } }), "success");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      reloadError = msg;
      addToast($t('triggers.reload_error', { values: { message: msg } }), "error");
    } finally {
      reloading = false;
    }
  }

  function handleFire(taskId: string) {
    addToast($t('triggers.fired_toast', { values: { taskId: taskId.slice(0, 8) } }), "success");
  }

  function handleOpenLogs(triggerId: string) {
    logsTriggerId = triggerId;
  }

  function handleCloseLogs() {
    logsTriggerId = null;
  }

  function handleEdit(triggerId: string) {
    editTriggerId = triggerId;
  }

  function handleCloseEdit() {
    editTriggerId = null;
  }

  function handleTriggerCreated(id: string) {
    addToast($t('triggers.created_toast', { values: { id } }), "success");
    void handleReload();
  }

  function handleTriggerUpdated(id: string) {
    addToast($t('triggers.updated_toast', { values: { id } }), "success");
    void handleReload();
  }

  function handleRequestDelete(triggerId: string) {
    deleteTriggerId = triggerId;
  }

  function handleCancelDelete() {
    deleteTriggerId = null;
  }

  async function handleConfirmDelete() {
    if (!deleteTriggerId) return;
    const id = deleteTriggerId;
    deleting = true;
    try {
      await invoke("delete_trigger", { id });
      deleteTriggerId = null;
      addToast($t('triggers.deleted_toast', { values: { id } }), "success");
      void handleReload();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      addToast($t('triggers.delete_error', { values: { message: msg } }), "error");
      deleteTriggerId = null;
    } finally {
      deleting = false;
    }
  }

  function navigateToAgents() {
    currentRoute.set("agents");
  }
</script>

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="triggers-page">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold" data-testid="triggers-header">{$t('triggers.title')}</h1>
      <p class="text-xs text-muted-foreground" data-testid="triggers-subtitle">{$t('triggers.subtitle')}</p>
    </div>
    <div class="flex items-center gap-2">
      {#if $uiMode === "builder"}
        <Button
          size="sm"
          onclick={() => (showCreateDialog = true)}
          data-testid="create-trigger-btn"
        >
          {$t('triggers.new_trigger')}
        </Button>
      {/if}
      <Button
        size="sm"
        variant="outline"
        onclick={handleReload}
        disabled={reloading}
        data-testid="triggers-reload-btn"
      >
        {reloading ? $t('triggers.reloading') : $t('triggers.reload')}
      </Button>
    </div>
  </div>

  <!-- Reload error -->
  {#if reloadError}
    <div
      class="rounded-md border border-destructive bg-destructive/10 px-4 py-2 text-sm text-destructive"
    >
      {reloadError}
    </div>
  {/if}

  <!-- Trigger list grouped by agent, or empty state -->
  {#if $triggers.length === 0}
    <EmptyState
      icon={EMPTY_STATES.triggers.icon}
      title={$t(EMPTY_STATES.triggers.titleKey)}
      description={$t(EMPTY_STATES.triggers.descriptionKey)}
      primaryLabel={$uiMode === "builder" ? $t(EMPTY_STATES.triggers.primaryCtaKey ?? '') : undefined}
      primaryAction={$uiMode === "builder" ? () => (showCreateDialog = true) : undefined}
      secondaryLabel={$t(EMPTY_STATES.triggers.secondaryCtaKey ?? '')}
      secondaryAction={handleReload}
      page="triggers"
    />
  {:else}
    <div class="space-y-6" data-testid="triggers-grouped">
      {#each [...triggersByAgent.entries()] as [agentName, agentTriggers] (agentName)}
        <section animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }} data-testid="trigger-group-{agentName}">
          <!-- Agent group header -->
          <button
            class="mb-3 flex items-center gap-2 text-left"
            onclick={navigateToAgents}
            data-testid="trigger-agent-link-{agentName}"
          >
            <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground" data-testid="trigger-group-header">{agentName}</h2>
            <span class="text-xs text-muted-foreground hover:underline">
              {$t('triggers.view_agent')} &rarr;
            </span>
          </button>

          <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2">
            {#each agentTriggers as trigger (trigger.id)}
              <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
              <TriggerRow
                {trigger}
                locale={$locale ?? "en"}
                isBuilder={$uiMode === "builder"}
                onfire={handleFire}
                onlogs={handleOpenLogs}
                onedit={handleEdit}
                ondelete={handleRequestDelete}
              />
              </div>
            {/each}
          </div>
        </section>
      {/each}
    </div>
  {/if}
</div>

<!-- Logs sheet -->
{#if logsTriggerId}
  <TriggerLogs
    triggerId={logsTriggerId}
    open={logsOpen}
    onclose={handleCloseLogs}
  />
{/if}

<!-- Create trigger dialog -->
<CreateTriggerDialog
  open={showCreateDialog}
  onclose={() => (showCreateDialog = false)}
  oncreated={handleTriggerCreated}
/>

<!-- Edit trigger dialog -->
{#if editTriggerId}
  <EditTriggerDialog
    open={editTriggerId !== null}
    triggerId={editTriggerId}
    onclose={handleCloseEdit}
    onupdated={handleTriggerUpdated}
  />
{/if}

<!-- Delete confirmation dialog -->
<ConfirmDialog
  open={deleteTriggerId !== null}
  onclose={handleCancelDelete}
  onconfirm={handleConfirmDelete}
  title={$t('triggers.delete_confirm_title')}
  message={$t('triggers.delete_confirm_message', { values: { id: deleteTriggerId ?? '' } })}
  confirmLabel={$t('triggers.delete_confirm_yes')}
  cancelLabel={$t('common.cancel')}
  loading={deleting}
  data-testid="delete-trigger-confirm"
/>
