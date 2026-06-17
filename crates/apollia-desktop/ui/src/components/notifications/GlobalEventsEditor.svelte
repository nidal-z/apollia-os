<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Check } from "lucide-svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import {
    NOTIFICATION_EVENT_TYPES,
    eventLabelKey,
    eventDescriptionKey,
  } from "$lib/notifications/event-labels";

  interface Props {
    onsaved: () => void;
  }

  let { onsaved }: Props = $props();

  let checkedEvents = $state<Set<string>>(new Set());
  let loading = $state(true);
  let savingEvent = $state<string | null>(null);

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

  // Optimistic toggle: flip locally, persist, revert on failure. No manual save
  // so the scope behaves like every other toggle in the app.
  async function toggleEvent(event: string): Promise<void> {
    const wasOn = checkedEvents.has(event);
    const next = new Set(checkedEvents);
    if (wasOn) next.delete(event);
    else next.add(event);
    checkedEvents = next;
    savingEvent = event;
    try {
      await invoke("set_notification_events", { events: [...next] });
      onsaved();
    } catch (err: unknown) {
      const reverted = new Set(checkedEvents);
      if (wasOn) reverted.add(event);
      else reverted.delete(event);
      checkedEvents = reverted;
      const message = err instanceof Error ? err.message : String(err);
      addToast($t("notifications.global_events_error", { values: { message } }), "error");
    } finally {
      savingEvent = null;
    }
  }

  void loadEvents();
</script>

<section data-testid="global-events-section">
  <h2 class="text-[13px] font-medium text-foreground">{$t("notifications.scope_title")}</h2>
  <p class="mb-2.5 mt-0.5 text-[11.5px] text-muted-foreground">{$t("notifications.scope_desc")}</p>

  {#if loading}
    <div class="flex flex-wrap gap-1.5">
      {#each { length: 5 } as _, i (i)}
        <div class="h-7 w-24 animate-pulse rounded-full bg-muted/40"></div>
      {/each}
    </div>
  {:else}
    <div class="flex flex-wrap gap-1.5">
      {#each NOTIFICATION_EVENT_TYPES as event (event)}
        {@const active = checkedEvents.has(event)}
        <button
          type="button"
          onclick={() => toggleEvent(event)}
          disabled={savingEvent === event}
          class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11.5px] transition-colors disabled:opacity-60 {active
            ? 'border-primary/40 bg-primary/10 text-primary'
            : 'border-border bg-surface-1 text-muted-foreground hover:bg-muted/50 hover:text-foreground'}"
          title={$t(eventDescriptionKey(event))}
          aria-pressed={active}
          data-testid="global-event-{event}"
        >
          {#if active}<Check size={11} class="shrink-0" />{/if}
          {$t(eventLabelKey(event))}
        </button>
      {/each}
    </div>
  {/if}
</section>
