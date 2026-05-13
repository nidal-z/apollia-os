<script lang="ts">
  import { t } from "svelte-i18n";
  import type { NotificationChannel, NotificationLogEntry } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Select } from "$lib/components/ui/select";
  import { eventLabelKey } from "$lib/notifications/event-labels";
  import { FormField } from "$lib/components/ui/form-field";
  import { DataTable, type Column } from "$lib/components/ui/data-table";

  interface Props {
    logs: NotificationLogEntry[];
    channels?: NotificationChannel[];
  }

  let { logs, channels = [] }: Props = $props();

  // Build a quick lookup of channel_id → label (only entries with a label).
  const channelLabels = $derived.by(() => {
    const map = new Map<string, string>();
    for (const ch of channels) {
      const lbl = ch.label?.trim();
      if (lbl) map.set(ch.channel_id, lbl);
    }
    return map;
  });

  function channelDisplay(id: string): string {
    return channelLabels.get(id) ?? id;
  }

  /**
   * Returns the humanized event label, falling back to the raw event name
   * when the translation is missing (e.g. `pipeline.*` events not in the
   * 6 globally-toggleable types).
   */
  function eventDisplay(event: string): string {
    const key = eventLabelKey(event);
    const translated = $t(key);
    return translated === key ? event : translated;
  }

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

  const columns = $derived<Column<NotificationLogEntry>[]>([
    { key: "sent_at", header: $t('notifications.table.timestamp'), cell: timestampCell },
    { key: "id", header: $t('notifications.table.channel'), cell: channelCell },
    { key: "event_name", header: $t('notifications.table.event'), cell: eventCell },
    { key: "error", header: $t('notifications.table.status'), cell: statusCell },
  ]);
</script>

{#snippet timestampCell(entry: NotificationLogEntry)}
  <span class="whitespace-nowrap text-[11px] text-muted-foreground">{formatRelativeTime(entry.sent_at)}</span>
{/snippet}

{#snippet channelCell(entry: NotificationLogEntry)}
  <div class="flex flex-wrap gap-1">
    {#each entryChannelIds(entry) as cid}
      <Badge variant="outline" class="text-[11px]" title={cid}>{channelDisplay(cid)}</Badge>
    {/each}
  </div>
{/snippet}

{#snippet eventCell(entry: NotificationLogEntry)}
  <span class="block text-[12px]">{eventDisplay(entry.event_name)}</span>
  <span class="block text-[10px] font-mono text-muted-foreground/70">{entry.event_name}</span>
{/snippet}

{#snippet statusCell(entry: NotificationLogEntry)}
  {@const status = entryStatus(entry)}
  {#if status === "sent"}
    <Badge variant="success">{$t('notifications.status.sent')}</Badge>
  {:else}
    <Badge variant="destructive">{$t('notifications.status.failed')}</Badge>
  {/if}
{/snippet}

<div class="space-y-3">
  <!-- Filter dropdown -->
  {#if channelIds.length > 0}
    <FormField
      inline
      id="channel-filter"
      label={$t('notifications.filter_by_channel')}
      labelClass="text-[11px] font-normal"
    >
      <Select
        id="channel-filter"
        class="h-8 w-auto text-xs"
        bind:value={filterChannel}
      >
        <option value="all">{$t('notifications.all_channels')}</option>
        {#each channelIds as cid}
          <option value={cid}>{channelDisplay(cid)}</option>
        {/each}
      </Select>
    </FormField>
  {/if}

  <!-- Table -->
  <DataTable
    data={filteredLogs}
    {columns}
    rowKey={(e) => e.id}
    class="min-w-[520px]"
    emptyLabel={$t('notifications.empty_history')}
  />
</div>
