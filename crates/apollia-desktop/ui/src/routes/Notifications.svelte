<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { NotificationChannel, NotificationLogEntry } from "$lib/types";
  import { Separator } from "$lib/components/ui/separator";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import NotificationChannelCard from "../components/notifications/NotificationChannelCard.svelte";
  import NotificationLog from "../components/notifications/NotificationLog.svelte";

  let channels = $state<NotificationChannel[]>([]);
  let logs = $state<NotificationLogEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  async function loadData() {
    loading = true;
    error = null;
    try {
      const [channelResult, logResult] = await Promise.all([
        invoke<NotificationChannel[]>("list_notification_channels"),
        invoke<NotificationLogEntry[]>("get_notification_logs", { limit: 50 }),
      ]);
      channels = channelResult;
      logs = logResult;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadData();
  });
</script>

<div class="space-y-6">
  <!-- Header -->
  <h1 class="text-2xl font-bold">{$t('notifications.title')}</h1>

  <!-- Loading -->
  {#if loading}
    <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2">
      <Skeleton width="100%" height="6rem" />
      <Skeleton width="100%" height="6rem" />
    </div>
  {:else if error}
    <div
      class="rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-4 py-2 text-sm text-[hsl(var(--destructive))]"
    >
      {error}
    </div>
  {:else if channels.length === 0}
    <!-- AC-4 — Empty state -->
    <div
      class="flex flex-col items-center justify-center gap-4 rounded-lg border border-dashed py-16"
    >
      <p class="text-muted-foreground">
        {$t('notifications.empty_channels')}
      </p>
    </div>
  {:else}
    <!-- AC-1 — Channels section -->
    <section>
      <h2 class="mb-3 text-lg font-semibold">{$t('notifications.channels_title')}</h2>
      <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2">
        {#each channels as channel (channel.channel_id)}
          <NotificationChannelCard {channel} />
        {/each}
      </div>
    </section>

    <Separator />

    <!-- AC-3 — History section -->
    <section>
      <h2 class="mb-3 text-lg font-semibold">{$t('notifications.history_title')}</h2>
      <NotificationLog {logs} />
    </section>
  {/if}
</div>
