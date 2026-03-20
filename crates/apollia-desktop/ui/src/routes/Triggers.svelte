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
  import { Timer } from "lucide-svelte";
  import TriggerRow from "../components/triggers/TriggerRow.svelte";
  import TriggerLogs from "../components/triggers/TriggerLogs.svelte";
  import CreateTriggerDialog from "../components/triggers/CreateTriggerDialog.svelte";
  import EditTriggerDialog from "../components/triggers/EditTriggerDialog.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  let reloading = $state(false);
  let reloadError = $state<string | null>(null);
  let toast = $state<{ message: string; type: "success" | "error" } | null>(
    null,
  );
  let toastTimer = $state<ReturnType<typeof setTimeout> | null>(null);

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

  function showToast(message: string, type: "success" | "error") {
    if (toastTimer !== null) {
      clearTimeout(toastTimer);
    }
    toast = { message, type };
    toastTimer = setTimeout(() => {
      toast = null;
      toastTimer = null;
    }, 4000);
  }

  async function handleReload() {
    reloading = true;
    reloadError = null;
    try {
      const result: TriggerReloadResult = await invoke("reload_triggers");
      showToast($t('triggers.reload_success', { values: { count: result.reloaded } }), "success");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      reloadError = msg;
      showToast($t('triggers.reload_error', { values: { message: msg } }), "error");
    } finally {
      reloading = false;
    }
  }

  function handleFire(taskId: string) {
    showToast($t('triggers.fired_toast', { values: { taskId: taskId.slice(0, 8) } }), "success");
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
    showToast($t('triggers.created_toast', { values: { id } }), "success");
    void handleReload();
  }

  function handleTriggerUpdated(id: string) {
    showToast($t('triggers.updated_toast', { values: { id } }), "success");
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
      showToast($t('triggers.deleted_toast', { values: { id } }), "success");
      void handleReload();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      showToast($t('triggers.delete_error', { values: { message: msg } }), "error");
      deleteTriggerId = null;
    } finally {
      deleting = false;
    }
  }

  function navigateToAgents() {
    currentRoute.set("agents");
  }
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="space-y-1">
      <h1 class="text-2xl font-bold" data-testid="triggers-header">{$t('triggers.title')}</h1>
      <p class="text-sm text-muted-foreground" data-testid="triggers-subtitle">{$t('triggers.subtitle')}</p>
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

  <!-- Toast -->
  {#if toast}
    <div
      class="rounded-md border px-4 py-2 text-sm {toast.type === 'success'
        ? 'border-[var(--apollia-success)] bg-[var(--apollia-success)]/10 text-[var(--apollia-success)]'
        : 'border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 text-[hsl(var(--destructive))]'}"
      data-testid="triggers-toast"
    >
      {toast.message}
    </div>
  {/if}

  <!-- Reload error -->
  {#if reloadError && !toast}
    <div
      class="rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-4 py-2 text-sm text-[hsl(var(--destructive))]"
    >
      {reloadError}
    </div>
  {/if}

  <!-- Trigger list grouped by agent, or empty state -->
  {#if $triggers.length === 0}
    <EmptyState
      icon={Timer}
      title={$t('triggers.empty_title')}
      subtitle={$t('triggers.empty_subtitle')}
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
            <h2 class="text-lg font-semibold">{agentName}</h2>
            <span class="text-xs text-muted-foreground hover:underline">
              {$t('triggers.view_agent')} &rarr;
            </span>
          </button>

          <div class="space-y-3">
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
{#if deleteTriggerId}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="button"
    tabindex="-1"
    onclick={handleCancelDelete}
    onkeydown={(e) => e.key === "Escape" && handleCancelDelete()}
  >
    <div
      class="w-[400px] rounded-lg border bg-background p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.key === "Escape" && handleCancelDelete()}
      data-testid="delete-trigger-dialog"
    >
      <h3 class="mb-2 text-lg font-semibold">{$t('triggers.delete_confirm_title')}</h3>
      <p class="mb-4 text-sm text-muted-foreground">
        {$t('triggers.delete_confirm_message', { values: { id: deleteTriggerId } })}
      </p>
      <div class="flex justify-end gap-2">
        <Button variant="outline" size="sm" onclick={handleCancelDelete} data-testid="delete-trigger-cancel-btn">
          {$t('common.cancel')}
        </Button>
        <Button
          size="sm"
          variant="destructive"
          onclick={handleConfirmDelete}
          disabled={deleting}
          data-testid="delete-trigger-confirm-btn"
        >
          {deleting ? $t('triggers.deleting') : $t('triggers.delete_confirm_yes')}
        </Button>
      </div>
    </div>
  </div>
{/if}
