<script lang="ts">
  /**
   * The last notifications Apollia sent, newest first: the event, the channels
   * it went through, when, and whether it arrived. Without any send yet, a
   * single sentence says so.
   */
  import { t, locale } from "svelte-i18n";
  import { History, Check, AlertTriangle } from "lucide-svelte";
  import type { NotificationChannel, NotificationLogEntry } from "$lib/types";
  import { formatRelativeTime } from "$lib/utils";
  import { eventLabelKey } from "$lib/notifications/event-labels";
  import { channelName, sendSucceeded } from "$lib/notifications/notificationsSection";
  import SettingsSection from "../settings/SettingsSection.svelte";

  interface Props {
    /** The last sends, `null` while they load. */
    sends: NotificationLogEntry[] | null;
    /** `true` when the sends could not be read. */
    failed: boolean;
    channels: NotificationChannel[];
  }

  let { sends, failed, channels }: Props = $props();

  function eventName(event: string): string {
    const key = eventLabelKey(event);
    const label = $t(key);
    return label === key ? event : label;
  }

  function channelsOf(entry: NotificationLogEntry): string {
    return Object.keys(entry.channels)
      .map((id) => {
        const known = channels.find((c) => c.channel_id === id);
        return known ? channelName(known) : id;
      })
      .join(", ");
  }
</script>

{#if sends !== null && sends.length === 0 && !failed}
  <p class="text-caption text-muted-foreground" data-testid="notifications-recent-empty">
    {$t("settings.notifications.recent_empty")}
  </p>
{:else}
  <SettingsSection
    title={$t("settings.notifications.recent_title")}
    bodyLayout="flush"
    data-testid="notifications-recent"
  >
    {#snippet icon()}<History size={15} strokeWidth={1.75} />{/snippet}
    {#if failed}
      <p class="px-4 pb-4 text-caption text-muted-foreground" data-testid="notifications-recent-error">
        {$t("settings.notifications.recent_error")}
      </p>
    {:else if sends !== null}
      <ul>
        {#each sends as entry (entry.id)}
          {@const ok = sendSucceeded(entry)}
          <li class="flex items-center gap-3 border-b border-border px-4 py-2.5 last:border-b-0" data-testid="notifications-recent-row">
            <div class="min-w-0 flex-1">
              <p class="truncate text-body-sm text-foreground">{eventName(entry.event_name)}</p>
              <p class="truncate text-caption text-muted-foreground">
                {channelsOf(entry)} · {formatRelativeTime(entry.sent_at, $locale ?? "en")}
              </p>
            </div>
            {#if ok}
              <span class="flex flex-none items-center gap-1 text-caption text-success-a11y">
                <Check size={12} strokeWidth={2} aria-hidden="true" />
                {$t("settings.notifications.recent_sent")}
              </span>
            {:else}
              <span class="flex flex-none items-center gap-1 text-caption text-destructive" title={entry.error ?? ""}>
                <AlertTriangle size={12} strokeWidth={2} aria-hidden="true" />
                {$t("settings.notifications.recent_failed")}
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </SettingsSection>
{/if}
