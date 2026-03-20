<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    onsaved: () => void;
  }

  let { onsaved }: Props = $props();

  const ALL_EVENT_TYPES = [
    "task.completed",
    "task.failed",
    "task.input_required",
    "agent.degraded",
    "trigger.error",
    "pipeline.completed",
    "pipeline.failed",
    "pipeline.suspended",
    "llm.backend_down",
  ] as const;

  let checkedEvents = $state<Set<string>>(new Set());
  let loading = $state(true);
  let saving = $state(false);
  let toast = $state<{ type: "success" | "error"; message: string } | null>(null);

  async function loadEvents(): Promise<void> {
    loading = true;
    try {
      const events: string[] = await invoke("get_notification_events");
      checkedEvents = new Set(events);
    } catch {
      checkedEvents = new Set();
    } finally {
      loading = false;
    }
  }

  function toggleEvent(event: string) {
    const next = new Set(checkedEvents);
    if (next.has(event)) {
      next.delete(event);
    } else {
      next.add(event);
    }
    checkedEvents = next;
  }

  async function handleSave(): Promise<void> {
    saving = true;
    toast = null;
    try {
      await invoke("set_notification_events", {
        events: [...checkedEvents],
      });
      toast = { type: "success", message: $t("notifications.global_events_saved") };
      onsaved();
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      toast = {
        type: "error",
        message: $t("notifications.global_events_error", { values: { message } }),
      };
    } finally {
      saving = false;
    }
  }

  void loadEvents();
</script>

<section data-testid="global-events-section">
  <h2 class="mb-1 text-lg font-semibold">{$t("notifications.global_events_title")}</h2>
  <p class="mb-3 text-sm text-muted-foreground">{$t("notifications.global_events_desc")}</p>

  {#if loading}
    <p class="text-sm text-muted-foreground">{$t("common.loading")}</p>
  {:else}
    <div class="grid grid-cols-2 gap-x-6 gap-y-2 sm:grid-cols-3">
      {#each ALL_EVENT_TYPES as event}
        <label class="flex items-center gap-2 text-sm" data-testid="global-event-{event}">
          <input
            type="checkbox"
            checked={checkedEvents.has(event)}
            onchange={() => toggleEvent(event)}
          />
          <span class="font-mono text-xs">{event}</span>
        </label>
      {/each}
    </div>

    <div class="mt-4 flex items-center gap-3">
      <Button
        size="sm"
        onclick={handleSave}
        disabled={saving}
        data-testid="global-events-save-btn"
      >
        {saving ? $t("notifications.global_events_saving") : $t("notifications.global_events_save")}
      </Button>

      {#if toast}
        {#if toast.type === "success"}
          <span class="text-sm text-[var(--apollia-success)]" data-testid="global-events-toast-success">
            {toast.message}
          </span>
        {:else}
          <span class="text-sm text-[hsl(var(--destructive))]" data-testid="global-events-toast-error">
            {toast.message}
          </span>
        {/if}
      {/if}
    </div>
  {/if}
</section>
