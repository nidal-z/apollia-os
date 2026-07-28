<script lang="ts">
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import type { NotificationChannel, NotificationChannelView } from "$lib/types";
  import { Separator } from "$lib/components/ui/separator";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { PageLayout, ErrorBanner } from "$lib/components/operator";
  import { EmptyState } from "$lib/components/layout";
  import { EMPTY_STATES } from "$lib/i18n/strings/empty-states";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import { humanize } from "$lib/errors/humanize";
  import { reportError } from "$lib/errors/reportError";
  import {
    listNotificationChannels,
    getNotificationEvents,
    deleteNotificationChannel,
  } from "$lib/ipc/notifications";
  import NotificationChannelCard from "../components/notifications/NotificationChannelCard.svelte";
  import GlobalEventsEditor from "../components/notifications/GlobalEventsEditor.svelte";
  import CreateChannelDialog from "../components/notifications/CreateChannelDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import EditChannelDialog from "../components/notifications/EditChannelDialog.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { navigateTo } from "$lib/stores/navigation";

  let channels = $state<NotificationChannel[]>([]);
  let globalEvents = $state<string[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let createDialogOpen = $state(false);
  let editDialogOpen = $state(false);
  let editingChannel = $state<NotificationChannelView | null>(null);

  let deleteConfirmId = $state<string | null>(null);
  let deleting = $state(false);

  async function loadData() {
    loading = true;
    error = null;
    try {
      const [channelResult, eventsResult] = await Promise.all([
        listNotificationChannels(),
        getNotificationEvents(),
      ]);
      channels = channelResult;
      globalEvents = eventsResult;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  /** Humanized view of a load failure (headline + guidance + raw detail). */
  const humanizedError = $derived(error ? humanize(error, (k) => $t(k)) : null);

  function openInboxNotificationsTab() {
    navigateTo("inbox");
    // The Inbox listens for this event to switch its active tab.
    window.dispatchEvent(
      new CustomEvent("apollia:inbox:select-tab", { detail: { tab: "notifications" } }),
    );
  }

  function handleChannelCreated(id: string) {
    addToast($t("notifications.created_toast", { values: { id } }), "success");
    void loadData();
  }

  function handleChannelUpdated(id: string) {
    addToast($t("notifications.updated_toast", { values: { id } }), "success");
    void loadData();
  }

  function handleEditChannel(ch: NotificationChannel) {
    editingChannel = {
      id: ch.channel_id,
      label: ch.label ?? null,
      channel_type: ch.type as "desktop" | "webhook",
      enabled: ch.enabled,
      config: {},
      events: ch.events.length > 0 ? ch.events : null,
      min_interval_seconds: ch.min_interval_seconds ?? 0,
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
      await deleteNotificationChannel(id);
      addToast($t("notifications.deleted_toast", { values: { id } }), "success");
      deleteConfirmId = null;
      void loadData();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
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

  onMount(() => {
    void loadData();
  });
</script>

<PageLayout
  title={$t("notifications.title")}
  subtitle={$t("notifications.subtitle")}
  data-testid="notifications-page"
>
  {#snippet actions()}
    <Button size="sm" onclick={() => (createDialogOpen = true)} data-testid="create-channel-btn">
      {$t("notifications.new_channel")}
    </Button>
  {/snippet}

  <div class="px-8 pt-6 pb-8 space-y-6">
    {#if loading}
      <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2">
        <Skeleton variant="card" height="6rem" />
        <Skeleton variant="card" height="6rem" />
      </div>
    {:else if humanizedError}
      <ErrorBanner
        tone="danger"
        onretry={() => void loadData()}
        retryLabel={$t("common.retry")}
        {loading}
        data-testid="notifications-error"
      >
        <p class="text-body-sm font-medium">{humanizedError.title}</p>
        <p class="text-body-xs opacity-80">
          {humanizedError.friendly_message} {humanizedError.suggested_action}
        </p>
        {#if humanizedError.detail}
          <div class="mt-2">
            <Disclosure testid="notifications-error-details">
              {#snippet summary()}{$t("notifications.error_details")}{/snippet}
              <pre class="whitespace-pre-wrap break-words font-mono text-caption text-destructive">{humanizedError.detail}</pre>
            </Disclosure>
          </div>
        {/if}
      </ErrorBanner>
    {:else}
      <!-- Global events section -->
      <GlobalEventsEditor onsaved={handleGlobalEventsSaved} />

      <Separator />

      <!-- Channels section -->
      <section>
        <h2 class="mb-3 text-body-sm font-medium text-foreground">{$t('notifications.channels_title')}</h2>
        {#if channels.length === 0}
          <EmptyState
            icon={EMPTY_STATES.notifications.icon}
            title={$t(EMPTY_STATES.notifications.titleKey)}
            description={$t(EMPTY_STATES.notifications.descriptionKey)}
            primaryLabel={$t(EMPTY_STATES.notifications.primaryCtaKey ?? '')}
            primaryAction={() => (createDialogOpen = true)}
            page="notifications"
          />
        {:else}
          <!-- Dense 3-column grid past 6 channels; rows animate on add/remove. -->
          <div
            class="grid gap-4 sm:grid-cols-1 {channels.length >= 6 ? 'md:grid-cols-3' : 'md:grid-cols-2'}"
            data-testid="notifications-channel-grid"
          >
            {#each channels as channel (channel.channel_id)}
              <div animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
                <NotificationChannelCard
                  {channel}
                  onedit={() => handleEditChannel(channel)}
                  ondelete={() => handleDeleteChannel(channel.channel_id)}
                />
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <!-- History moved to the Inbox -->
      <p class="text-xs text-muted-foreground" data-testid="notifications-history-moved">
        {$t("notifications.history_moved")}
        <Button variant="ghost" size="sm"
          type="button"
          class="ml-1 text-primary underline-offset-2 hover:underline"
          onclick={openInboxNotificationsTab}
          data-testid="notifications-history-link"
        >
          {$t("notifications.history_open_inbox")}
        </Button>
      </p>
    {/if}
  </div>
</PageLayout>

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

<!-- Delete confirmation dialog -->
<ConfirmDialog
  open={deleteConfirmId !== null}
  onclose={cancelDelete}
  onconfirm={confirmDelete}
  title={$t("notifications.delete_confirm_title")}
  message={$t("notifications.delete_confirm_message", { values: { id: deleteConfirmId ?? '' } })}
  confirmLabel={$t("notifications.delete_confirm_yes")}
  cancelLabel={$t("common.cancel")}
  loading={deleting}
  data-testid="delete-channel-confirm"
/>
