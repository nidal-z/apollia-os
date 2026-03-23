<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { TriggerLogEntry } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    triggerId: string;
    open: boolean;
    onclose: () => void;
  }

  let { triggerId, open, onclose }: Props = $props();

  let entries = $state<TriggerLogEntry[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let loaded = $state(false);

  const STATUS_CONFIG: Record<
    string,
    {
      label: string;
      variant: "success" | "warning" | "destructive" | "secondary";
    }
  > = {
    fired: { label: "FIRED", variant: "success" },
    skipped: { label: "SKIPPED", variant: "warning" },
    error: { label: "ERROR", variant: "destructive" },
  };

  async function loadLogs() {
    loading = true;
    error = null;
    try {
      const result: TriggerLogEntry[] = await invoke("get_trigger_logs", {
        id: triggerId,
        limit: 20,
      });
      entries = result;
      loaded = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function formatTimestamp(iso: string): string {
    const date = new Date(iso);
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  $effect(() => {
    if (open && !loaded) {
      void loadLogs();
    }
  });

  function handleBackdropClick() {
    onclose();
  }

  function handlePanelClick(event: MouseEvent) {
    event.stopPropagation();
  }
</script>

{#if open}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex justify-end bg-black/40 backdrop-blur-sm"
    onclick={handleBackdropClick}
  >
    <!-- Sheet panel -->
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="h-full w-full max-w-md overflow-auto border-l glass-panel glass-border p-6 shadow-lg"
      onclick={handlePanelClick}
    >
      <!-- Header -->
      <div class="mb-4 flex items-center justify-between">
        <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {$t('triggers.logs_title')} — <code class="text-[11px] normal-case">{triggerId}</code>
        </h2>
        <Button size="sm" variant="ghost" onclick={onclose}>✕</Button>
      </div>

      <!-- Content -->
      {#if loading}
        <p class="text-[11px] text-muted-foreground">{$t('common.loading')}</p>
      {:else if error}
        <div
          class="rounded-md border border-destructive bg-destructive/10 px-4 py-2 text-[11px] text-destructive"
        >
          {error}
        </div>
      {:else if entries.length === 0}
        <p class="text-[11px] text-muted-foreground">
          {$t('triggers.no_logs')}
        </p>
      {:else}
        <div class="glass-card glass-border rounded-lg overflow-hidden" data-testid="trigger-logs-table">
          <table class="w-full text-[13px]">
            <thead class="border-b border-border bg-muted/50">
              <tr>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('triggers.status')}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('triggers.agent_label')}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('triggers.task_label')}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t('triggers.time')}</th>
              </tr>
            </thead>
            <tbody>
              {#each entries as entry (entry.id)}
                {@const statusConfig = STATUS_CONFIG[entry.status] ?? STATUS_CONFIG["error"]}
                <tr class="border-b border-border last:border-0 hover:bg-muted/50">
                  <td class="px-3 py-2">
                    <Badge variant={statusConfig.variant}>
                      {statusConfig.label}
                    </Badge>
                  </td>
                  <td class="px-3 py-2 text-[11px] text-muted-foreground">{entry.agent_name}</td>
                  <td class="px-3 py-2 text-[11px] text-muted-foreground">
                    {#if entry.task_id}
                      <code>{entry.task_id.slice(0, 8)}</code>
                    {:else}
                      —
                    {/if}
                  </td>
                  <td class="px-3 py-2 text-[11px] text-muted-foreground">{formatTimestamp(entry.fired_at)}</td>
                </tr>
                {#if entry.reason}
                  <tr class="border-b border-border last:border-0">
                    <td colspan="4" class="px-3 pb-2 text-[11px] text-destructive">{entry.reason}</td>
                  </tr>
                {/if}
              {/each}
            </tbody>
          </table>
        </div>
      {/if}

      <!-- Refresh -->
      <div class="mt-4">
        <Button
          size="sm"
          variant="outline"
          onclick={loadLogs}
          disabled={loading}
        >
          {loading ? $t('common.loading') : $t('common.refresh')}
        </Button>
      </div>
    </div>
  </div>
{/if}
