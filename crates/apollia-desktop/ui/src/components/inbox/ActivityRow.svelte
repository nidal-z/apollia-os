<script lang="ts">
  /**
   * One activity entry in the Inbox "Activité" tab.
   *
   * Picks an icon/color from the event name, surfaces the human-readable
   * label via `eventLabelKey`, and exposes a "Voir les logs" link that
   * jumps to the most-relevant page for follow-up.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle, XCircle, Zap, type Icon } from "lucide-svelte";
  import { BtnSecondary } from "$lib/components/operator";
  import { eventLabelKey } from "$lib/notifications/event-labels";
  import { formatRelativeTime } from "$lib/utils";
  import { navigateTo } from "$lib/stores/navigation";
  import type { NotificationLogEntry } from "$lib/types";

  interface Props {
    entry: NotificationLogEntry;
  }

  let { entry }: Props = $props();

  // Visual mapping for the 4 activity event names. Falls back to a generic
  // warning icon for anything else that slips through the backend filter.
  const visual = $derived(eventVisual(entry.event_name));

  function eventVisual(event: string): { icon: typeof Icon; tone: string } {
    switch (event) {
      case "task.failed":
      case "trigger.error":
        return { icon: XCircle, tone: "text-destructive" };
      case "agent.degraded":
        return { icon: AlertTriangle, tone: "text-amber-500" };
      case "llm.backend_down":
        return { icon: Zap, tone: "text-destructive" };
      default:
        return { icon: AlertTriangle, tone: "text-muted-foreground" };
    }
  }

  function handleSeeLogs() {
    // `agent_id` n'est pas persisté dans notification_logs, donc on retombe
    // sur Observabilité → Chronologie qui filtre par task_id si dispo.
    if (entry.task_id) {
      navigateTo("observability");
      window.dispatchEvent(
        new CustomEvent("apollia:observability:focus-task", {
          detail: { task_id: entry.task_id },
        }),
      );
    } else {
      navigateTo("observability");
    }
  }
</script>

<div
  class="flex items-center gap-3 rounded-md border border-border/60 bg-background/60 px-3 py-2.5"
  data-testid="activity-row"
  data-event={entry.event_name}
>
  <span class={visual.tone} aria-hidden="true">
    <visual.icon size={18} strokeWidth={2} />
  </span>

  <div class="flex min-w-0 flex-1 flex-col gap-0.5">
    <div class="flex items-center justify-between gap-2">
      <span class="text-sm font-medium text-foreground">
        {$t(eventLabelKey(entry.event_name))}
      </span>
      <span class="shrink-0 text-[11px] text-muted-foreground">
        {formatRelativeTime(entry.sent_at)}
      </span>
    </div>
    {#if entry.task_id}
      <span class="truncate text-xs text-muted-foreground" title={entry.task_id}>
        {entry.task_id}
      </span>
    {/if}
    {#if entry.error}
      <span class="text-xs text-destructive/80">{entry.error}</span>
    {/if}
  </div>

  <BtnSecondary onclick={handleSeeLogs}>
    {$t("inbox.activity.see_logs")}
  </BtnSecondary>
</div>
