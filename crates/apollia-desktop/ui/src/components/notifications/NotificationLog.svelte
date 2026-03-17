<script lang="ts">
  import { t } from "svelte-i18n";
  import type { NotificationLogEntry } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";

  interface Props {
    logs: NotificationLogEntry[];
  }

  let { logs }: Props = $props();

  let filterChannel = $state("all");

  const channelIds = $derived(
    [...new Set(logs.flatMap((entry) => Object.keys(entry.channels)))].sort(),
  );

  const filteredLogs = $derived(
    filterChannel === "all"
      ? logs
      : logs.filter((entry) => filterChannel in entry.channels),
  );

  function formatRelativeTime(iso: string): string {
    const diff = Date.now() - new Date(iso).getTime();
    const seconds = Math.floor(diff / 1000);
    if (seconds < 60) return `${seconds}s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }

  function entryStatus(
    entry: NotificationLogEntry,
  ): "sent" | "failed" {
    if (entry.error) return "failed";
    const statuses = Object.values(entry.channels);
    if (statuses.some((s) => s === "error")) return "failed";
    return "sent";
  }

  function entryChannelIds(entry: NotificationLogEntry): string[] {
    return Object.keys(entry.channels);
  }
</script>

<div class="space-y-3">
  <!-- Filter dropdown -->
  {#if channelIds.length > 0}
    <div class="flex items-center gap-2">
      <label for="channel-filter" class="text-sm text-muted-foreground">
        {$t('notifications.filter_by_channel')}
      </label>
      <select
        id="channel-filter"
        class="rounded-md border bg-background px-3 py-1.5 text-sm"
        bind:value={filterChannel}
      >
        <option value="all">{$t('notifications.all_channels')}</option>
        {#each channelIds as cid}
          <option value={cid}>{cid}</option>
        {/each}
      </select>
    </div>
  {/if}

  <!-- Table -->
  {#if filteredLogs.length === 0}
    <div
      class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12"
    >
      <p class="text-muted-foreground">{$t('notifications.empty_history')}</p>
    </div>
  {:else}
    <div class="overflow-auto rounded-md border">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b glass-border-subtle glass-surface">
            <th class="px-3 py-2 text-left font-medium text-muted-foreground">
              {$t('notifications.table.timestamp')}
            </th>
            <th class="px-3 py-2 text-left font-medium text-muted-foreground">
              {$t('notifications.table.channel')}
            </th>
            <th class="px-3 py-2 text-left font-medium text-muted-foreground">
              {$t('notifications.table.event')}
            </th>
            <th class="px-3 py-2 text-left font-medium text-muted-foreground">
              {$t('notifications.table.status')}
            </th>
          </tr>
        </thead>
        <tbody>
          {#each filteredLogs as entry (entry.id)}
            {@const status = entryStatus(entry)}
            <tr class="border-b last:border-b-0">
              <td class="whitespace-nowrap px-3 py-2 text-xs text-muted-foreground">
                {formatRelativeTime(entry.sent_at)}
              </td>
              <td class="px-3 py-2">
                <div class="flex flex-wrap gap-1">
                  {#each entryChannelIds(entry) as cid}
                    <Badge variant="outline" class="text-xs">{cid}</Badge>
                  {/each}
                </div>
              </td>
              <td class="px-3 py-2 text-xs">{entry.event_name}</td>
              <td class="px-3 py-2">
                {#if status === "sent"}
                  <Badge
                    variant="default"
                    class="bg-[var(--apollia-success)] text-white"
                  >
                    {$t('notifications.status.sent')}
                  </Badge>
                {:else}
                  <Badge variant="destructive">{$t('notifications.status.failed')}</Badge>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
