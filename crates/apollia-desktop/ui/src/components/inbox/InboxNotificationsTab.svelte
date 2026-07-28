<script lang="ts">
  /**
   * "Notifications" tab: the sent-notification log (desktop, webhook, ...).
   * A failed load surfaces a humanized error box instead of a blank table, and
   * a channel-list failure is shown without hiding the log rows that did load.
   */
  import { t } from "svelte-i18n";
  import { Bell } from "lucide-svelte";
  import { EmptyState, SkeletonList } from "$lib/components/operator";
  import NotificationLog from "../notifications/NotificationLog.svelte";
  import InboxErrorBox from "./InboxErrorBox.svelte";
  import type { NotificationChannel, NotificationLogEntry } from "$lib/types";

  interface Props {
    logs: NotificationLogEntry[];
    channels: NotificationChannel[];
    loading: boolean;
    error: unknown;
    channelsError: unknown;
    onRetry: () => void;
  }

  let { logs, channels, loading, error, channelsError, onRetry }: Props = $props();
</script>

<div class="min-h-0 flex-1 overflow-y-auto px-8 pb-10 pt-3">
  {#if loading}
    <SkeletonList count={5} avatar={false} />
  {:else if error}
    <InboxErrorBox {error} {onRetry} testid="inbox-notifications-error" />
  {:else if logs.length === 0}
    <div class="py-10">
      <EmptyState title={$t("inbox.notifications.empty")} desc={$t("inbox.notifications.empty_desc")} tone="neutral">
        {#snippet icon()}<Bell size={22} />{/snippet}
      </EmptyState>
    </div>
  {:else}
    {#if channelsError}
      <div class="mb-3">
        <InboxErrorBox error={channelsError} {onRetry} testid="inbox-channels-error" />
      </div>
    {/if}
    <NotificationLog {logs} {channels} />
  {/if}
</div>
