<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { NotificationChannel, NotificationLogEntry, NotificationChannelView } from "$lib/types";
  import { Separator } from "$lib/components/ui/separator";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Button } from "$lib/components/ui/button";
  import NotificationChannelCard from "../components/notifications/NotificationChannelCard.svelte";
  import NotificationLog from "../components/notifications/NotificationLog.svelte";
  import GlobalEventsEditor from "../components/notifications/GlobalEventsEditor.svelte";
  import CreateChannelDialog from "../components/notifications/CreateChannelDialog.svelte";
  import EditChannelDialog from "../components/notifications/EditChannelDialog.svelte";

  let channels = $state<NotificationChannel[]>([]);
  let logs = $state<NotificationLogEntry[]>([]);
  let globalEvents = $state<string[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let createDialogOpen = $state(false);
  let editDialogOpen = $state(false);
  let editingChannel = $state<NotificationChannelView | null>(null);

  let deleteConfirmId = $state<string | null>(null);
  let deleting = $state(false);

  let toast = $state<{ type: "success" | "error"; message: string } | null>(null);
  let toastTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  function showToast(type: "success" | "error", message: string) {
    if (toastTimer !== null) {
      clearTimeout(toastTimer);
    }
    toast = { type, message };
    toastTimer = setTimeout(() => {
      toast = null;
      toastTimer = null;
    }, 4_000);
  }

  async function loadData() {
    loading = true;
    error = null;
    try {
      const [channelResult, logResult, eventsResult] = await Promise.all([
        invoke<NotificationChannel[]>("list_notification_channels"),
        invoke<NotificationLogEntry[]>("get_notification_logs", { limit: 50 }),
        invoke<string[]>("get_notification_events"),
      ]);
      channels = channelResult;
      logs = logResult;
      globalEvents = eventsResult;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function handleChannelCreated(id: string) {
    showToast("success", $t("notifications.created_toast", { values: { id } }));
    void loadData();
  }

  function handleChannelUpdated(id: string) {
    showToast("success", $t("notifications.updated_toast", { values: { id } }));
    void loadData();
  }

  function handleEditChannel(ch: NotificationChannel) {
    editingChannel = {
      id: ch.channel_id,
      channel_type: ch.type as "desktop" | "webhook",
      enabled: ch.enabled,
      config: {},
      events: ch.events.length > 0 ? ch.events : null,
      created_at: "",
      updated_at: "",
    };
    editDialogOpen = true;
  }

  function handleDeleteChannel(id: string) {
    deleteConfirmId = id;
  }

  async function confirmDelete() {
    if (!deleteConfirmId) return;
    const id = deleteConfirmId;
    deleting = true;
    try {
      await invoke("delete_notification_channel", { id });
      showToast("success", $t("notifications.deleted_toast", { values: { id } }));
      deleteConfirmId = null;
      void loadData();
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      showToast("error", $t("notifications.delete_error", { values: { message } }));
    } finally {
      deleting = false;
    }
  }

  function cancelDelete() {
    deleteConfirmId = null;
  }

  function handleGlobalEventsSaved() {
    void loadData();
  }

  function handleDeleteKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      cancelDelete();
    }
  }

  onMount(() => {
    void loadData();
  });
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="space-y-1">
      <h1 class="text-2xl font-semibold">{$t('notifications.title')}</h1>
      <p class="text-sm text-muted-foreground" data-testid="notifications-subtitle">{$t('notifications.subtitle')}</p>
    </div>
    <Button size="sm" onclick={() => (createDialogOpen = true)} data-testid="create-channel-btn">
      {$t('notifications.new_channel')}
    </Button>
  </div>

  <!-- Toast -->
  {#if toast}
    <div
      class="rounded-md px-4 py-2 text-sm {toast.type === 'success'
        ? 'border border-[var(--apollia-success)] bg-[var(--apollia-success)]/10 text-[var(--apollia-success)]'
        : 'border border-destructive bg-destructive/10 text-destructive'}"
      data-testid="notifications-toast"
    >
      {toast.message}
    </div>
  {/if}

  <!-- Loading -->
  {#if loading}
    <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2">
      <Skeleton width="100%" height="6rem" />
      <Skeleton width="100%" height="6rem" />
    </div>
  {:else if error}
    <div
      class="rounded-md border border-destructive bg-destructive/10 px-4 py-2 text-sm text-destructive"
    >
      {error}
    </div>
  {:else}
    <!-- AC-1 — Global events section -->
    <GlobalEventsEditor onsaved={handleGlobalEventsSaved} />

    <Separator />

    <!-- AC-2 / AC-6 — Channels section -->
    <section>
      <h2 class="mb-3 text-lg font-medium">{$t('notifications.channels_title')}</h2>
      {#if channels.length === 0}
        <div
          class="flex flex-col items-center justify-center gap-4 rounded-xl glass-surface glass-border border-dashed py-16"
        >
          <p class="text-muted-foreground">
            {$t('notifications.empty_channels')}
          </p>
          <Button size="sm" variant="outline" onclick={() => (createDialogOpen = true)} data-testid="create-channel-empty-btn">
            {$t('notifications.new_channel')}
          </Button>
        </div>
      {:else}
        <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2">
          {#each channels as channel (channel.channel_id)}
            <NotificationChannelCard
              {channel}
              onedit={() => handleEditChannel(channel)}
              ondelete={() => handleDeleteChannel(channel.channel_id)}
            />
          {/each}
        </div>
      {/if}
    </section>

    <Separator />

    <!-- History section -->
    <section>
      <h2 class="mb-3 text-lg font-medium">{$t('notifications.history_title')}</h2>
      <NotificationLog {logs} />
    </section>
  {/if}
</div>

<!-- Dialogs -->
<CreateChannelDialog
  open={createDialogOpen}
  {globalEvents}
  onclose={() => (createDialogOpen = false)}
  oncreated={handleChannelCreated}
/>

<EditChannelDialog
  open={editDialogOpen}
  channel={editingChannel}
  {globalEvents}
  onclose={() => (editDialogOpen = false)}
  onupdated={handleChannelUpdated}
/>

<!-- Delete confirmation dialog (AC-6) -->
{#if deleteConfirmId}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="button"
    tabindex="-1"
    onclick={cancelDelete}
    onkeydown={handleDeleteKeydown}
  >
    <div
      class="w-[400px] rounded-lg border bg-background p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleDeleteKeydown}
      data-testid="delete-channel-dialog"
    >
      <h3 class="mb-2 text-lg font-medium">{$t("notifications.delete_confirm_title")}</h3>
      <p class="mb-4 text-sm text-muted-foreground">
        {$t("notifications.delete_confirm_message", { values: { id: deleteConfirmId } })}
      </p>
      <div class="flex justify-end gap-2">
        <Button variant="outline" size="sm" onclick={cancelDelete} data-testid="delete-channel-cancel-btn">
          {$t("common.cancel")}
        </Button>
        <Button
          size="sm"
          variant="destructive"
          onclick={confirmDelete}
          disabled={deleting}
          data-testid="delete-channel-confirm-btn"
        >
          {deleting ? $t("notifications.deleting") : $t("notifications.delete_confirm_yes")}
        </Button>
      </div>
    </div>
  </div>
{/if}
