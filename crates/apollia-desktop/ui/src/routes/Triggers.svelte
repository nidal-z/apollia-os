<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { triggers } from "$lib/stores/triggers";
  import type { TriggerReloadResult } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import TriggerRow from "../components/triggers/TriggerRow.svelte";
  import TriggerLogs from "../components/triggers/TriggerLogs.svelte";

  let reloading = $state(false);
  let reloadError = $state<string | null>(null);
  let toast = $state<{ message: string; type: "success" | "error" } | null>(
    null,
  );
  let toastTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  let logsTriggerId = $state<string | null>(null);
  let logsOpen = $derived(logsTriggerId !== null);

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
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <h1 class="text-2xl font-bold">{$t('triggers.title')}</h1>
    <Button
      size="sm"
      variant="outline"
      onclick={handleReload}
      disabled={reloading}
    >
      {reloading ? $t('triggers.reloading') : $t('triggers.reload')}
    </Button>
  </div>

  <!-- Toast -->
  {#if toast}
    <div
      class="rounded-md border px-4 py-2 text-sm {toast.type === 'success'
        ? 'border-[var(--apollia-success)] bg-[var(--apollia-success)]/10 text-[var(--apollia-success)]'
        : 'border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 text-[hsl(var(--destructive))]'}"
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

  <!-- Trigger list or empty state -->
  {#if $triggers.length === 0}
    <div
      class="flex flex-col items-center justify-center gap-4 rounded-lg border border-dashed py-16"
    >
      <p class="text-muted-foreground">
        {$t('triggers.empty')}
      </p>
    </div>
  {:else}
    <div class="space-y-3">
      {#each $triggers as trigger (trigger.id)}
        <TriggerRow
          {trigger}
          onfire={handleFire}
          onlogs={handleOpenLogs}
        />
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
