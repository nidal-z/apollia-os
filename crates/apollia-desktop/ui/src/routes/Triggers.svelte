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
  import EmptyState from "../components/common/EmptyState.svelte";

  let reloading = $state(false);
  let reloadError = $state<string | null>(null);
  let toast = $state<{ message: string; type: "success" | "error" } | null>(
    null,
  );
  let toastTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  let logsTriggerId = $state<string | null>(null);
  let logsOpen = $derived(logsTriggerId !== null);

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
