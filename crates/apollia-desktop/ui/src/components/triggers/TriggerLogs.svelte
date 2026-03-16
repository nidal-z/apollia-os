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
      variant: "default" | "secondary" | "destructive" | "outline";
      extraClass: string;
    }
  > = {
    fired: {
      label: "FIRED",
      variant: "default",
      extraClass: "bg-[var(--apollia-success)] text-white",
    },
    skipped: {
      label: "SKIPPED",
      variant: "outline",
      extraClass: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]",
    },
    error: { label: "ERROR", variant: "destructive", extraClass: "" },
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
    class="fixed inset-0 z-50 flex justify-end bg-black/50"
    onclick={handleBackdropClick}
  >
    <!-- Sheet panel -->
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="h-full w-full max-w-md overflow-auto border-l bg-background p-6 shadow-lg"
      onclick={handlePanelClick}
    >
      <!-- Header -->
      <div class="mb-4 flex items-center justify-between">
        <h2 class="text-lg font-semibold">
          {$t('triggers.logs_title')} — <code class="text-sm">{triggerId}</code>
        </h2>
        <Button size="sm" variant="ghost" onclick={onclose}>✕</Button>
      </div>

      <!-- Content -->
      {#if loading}
        <p class="text-sm text-muted-foreground">{$t('common.loading')}</p>
      {:else if error}
        <div
          class="rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-4 py-2 text-sm text-[hsl(var(--destructive))]"
        >
          {error}
        </div>
      {:else if entries.length === 0}
        <p class="text-sm text-muted-foreground">
          {$t('triggers.no_logs')}
        </p>
      {:else}
        <div class="space-y-2">
          {#each entries as entry (entry.id)}
            {@const statusConfig = STATUS_CONFIG[entry.status] ?? STATUS_CONFIG["error"]}
            <div class="rounded-md border px-3 py-2">
              <div class="flex items-center justify-between">
                <Badge
                  variant={statusConfig.variant}
                  class={statusConfig.extraClass}
                >
                  {statusConfig.label}
                </Badge>
                <span class="text-xs text-muted-foreground">
                  {formatTimestamp(entry.fired_at)}
                </span>
              </div>
              <div class="mt-1 text-xs text-muted-foreground">
                {$t('triggers.agent_label')}: {entry.agent_name}
                {#if entry.task_id}
                  <span class="ml-2">
                    {$t('triggers.task_label')}: <code>{entry.task_id.slice(0, 8)}</code>
                  </span>
                {/if}
              </div>
              {#if entry.reason}
                <div class="mt-1 text-xs text-[hsl(var(--destructive))]">
                  {entry.reason}
                </div>
              {/if}
            </div>
          {/each}
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
