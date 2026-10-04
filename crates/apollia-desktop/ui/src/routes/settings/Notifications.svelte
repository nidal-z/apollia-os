<script lang="ts">
  /**
   * Settings > Notifications: where Apollia warns the user (the channels) and
   * about what (the notified events), then the last sends. Every change is
   * saved at once, so the section has no save footer.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Bell, Plus, Send } from "lucide-svelte";
  import type { ChannelTestResult, NotificationChannel, NotificationChannelView, NotificationLogEntry } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { EmptyState, ErrorBanner } from "$lib/components/operator";
  import { addToast } from "$lib/components/ui/toast/store";
  import { humanize } from "$lib/errors/humanize";
  import { reportError } from "$lib/errors/reportError";
  import { EMPTY_STATES } from "$lib/i18n/strings/empty-states";
  import { viewFromListed } from "$lib/notifications/channelForm";
  import {
    channelName,
    loadChannels,
    loadNotifiedEvents,
    loadRecentSends,
    removeChannel,
    runChannelTest,
    setChannelEnabled,
    toggleNotifiedEvent,
  } from "$lib/notifications/notificationsSection";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import ChannelRow from "../../components/notifications/ChannelRow.svelte";
  import ChannelDialog from "../../components/notifications/ChannelDialog.svelte";
  import NotifiedEvents from "../../components/notifications/NotifiedEvents.svelte";
  import RecentSends from "../../components/notifications/RecentSends.svelte";

  let channels = $state<NotificationChannel[]>([]);
  let loadingChannels = $state(true);
  let channelsError = $state<string | null>(null);

  let notified = $state<string[] | null>(null);
  let savingEvent = $state<string | null>(null);

  let sends = $state<NotificationLogEntry[] | null>(null);
  let sendsFailed = $state(false);

  let testing = $state<Record<string, boolean>>({});
  let results = $state<Record<string, ChannelTestResult>>({});
  let toggling = $state<Record<string, boolean>>({});

  let dialogOpen = $state(false);
  let editing = $state<NotificationChannelView | null>(null);

  let deleting = $state<NotificationChannel | null>(null);
  let deleteBusy = $state(false);

  const humanizedError = $derived(channelsError ? humanize(channelsError, (k) => $t(k)) : null);

  async function refreshChannels(): Promise<void> {
    loadingChannels = true;
    channelsError = null;
    try {
      channels = await loadChannels();
    } catch (err: unknown) {
      channelsError = err instanceof Error ? err.message : String(err);
    } finally {
      loadingChannels = false;
    }
  }

  async function refreshEvents(): Promise<void> {
    try {
      notified = await loadNotifiedEvents();
    } catch (err: unknown) {
      notified = [];
      reportError(err, { surface: "toast" });
    }
  }

  async function refreshSends(): Promise<void> {
    try {
      sends = await loadRecentSends();
      sendsFailed = false;
    } catch {
      sends = [];
      sendsFailed = true;
    }
  }

  function openAdd(): void {
    editing = null;
    dialogOpen = true;
  }

  function openEdit(channel: NotificationChannel): void {
    editing = viewFromListed(channel);
    dialogOpen = true;
  }

  function onSaved(name: string): void {
    const key = editing ? "settings.notifications.updated_toast" : "settings.notifications.created_toast";
    addToast($t(key, { values: { name } }), "success");
    void refreshChannels();
  }

  async function test(channel: NotificationChannel): Promise<void> {
    const id = channel.channel_id;
    if (testing[id] || !channel.enabled) return;
    testing = { ...testing, [id]: true };
    const { [id]: _previous, ...rest } = results;
    results = rest;
    const outcome = await runChannelTest(id);
    results = { ...results, [id]: outcome };
    testing = { ...testing, [id]: false };
    void refreshSends();
  }

  async function toggle(channel: NotificationChannel, enabled: boolean): Promise<void> {
    const id = channel.channel_id;
    if (toggling[id]) return;
    toggling = { ...toggling, [id]: true };
    const previous = channel.enabled;
    channels = channels.map((c) => (c.channel_id === id ? { ...c, enabled } : c));
    try {
      await setChannelEnabled(id, enabled);
    } catch (err: unknown) {
      channels = channels.map((c) => (c.channel_id === id ? { ...c, enabled: previous } : c));
      reportError(err, { surface: "toast" });
    } finally {
      toggling = { ...toggling, [id]: false };
    }
  }

  async function confirmDelete(): Promise<void> {
    const target = deleting;
    if (!target) return;
    deleteBusy = true;
    try {
      await removeChannel(target.channel_id);
      addToast($t("settings.notifications.deleted_toast", { values: { name: channelName(target) } }), "success");
      deleting = null;
      void refreshChannels();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      deleteBusy = false;
    }
  }

  async function toggleEvent(event: string): Promise<void> {
    if (notified === null || savingEvent) return;
    savingEvent = event;
    try {
      notified = await toggleNotifiedEvent(notified, event);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      addToast($t("notifications.global_events_error", { values: { message } }), "error");
    } finally {
      savingEvent = null;
    }
  }

  onMount(() => {
    void refreshChannels();
    void refreshEvents();
    void refreshSends();
  });
</script>

<SettingsSubPage route="notifications" data-testid="notifications-page">
  <p class="text-body-sm text-muted-foreground" data-testid="notifications-intro">
    {$t("settings.notifications.intro")}
  </p>

  <SettingsSection
    title={$t("notifications.channels_title")}
    bodyLayout="flush"
    data-testid="notifications-channels"
  >
    {#snippet icon()}<Send size={15} strokeWidth={1.75} />{/snippet}
    {#snippet actions()}
      {#if !loadingChannels && !humanizedError && channels.length > 0}
        <Button size="sm" variant="outline" onclick={openAdd} data-testid="create-channel-btn">
          {#snippet icon()}<Plus size={14} strokeWidth={1.75} />{/snippet}
          {$t("settings.notifications.add_channel")}
        </Button>
      {/if}
    {/snippet}

    {#if loadingChannels}
      <div class="space-y-2 px-4 pb-4">
        <Skeleton variant="table-row" />
        <Skeleton variant="table-row" />
      </div>
    {:else if humanizedError}
      <div class="px-4 pb-4">
        <ErrorBanner
          tone="danger"
          onretry={() => void refreshChannels()}
          retryLabel={$t("common.retry")}
          data-testid="notifications-error"
        >
          <p class="text-body-sm font-medium">{$t("settings.notifications.channels_error")}</p>
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
      </div>
    {:else if channels.length === 0}
      <div class="px-4 pb-4" data-testid="empty-state-{EMPTY_STATES.notifications.page}">
        <EmptyState
          title={$t(EMPTY_STATES.notifications.titleKey)}
          desc={$t(EMPTY_STATES.notifications.descriptionKey)}
          tone="neutral"
        >
          {#snippet icon()}<Bell size={22} strokeWidth={1.75} />{/snippet}
          {#snippet action()}
            <Button size="sm" onclick={openAdd} data-testid="create-channel-btn">
              {$t(EMPTY_STATES.notifications.primaryCtaKey ?? "settings.notifications.add_channel")}
            </Button>
          {/snippet}
        </EmptyState>
      </div>
    {:else}
      <ul data-testid="notifications-channel-grid">
        {#each channels as channel (channel.channel_id)}
          <ChannelRow
            {channel}
            testing={testing[channel.channel_id] === true}
            result={results[channel.channel_id] ?? null}
            toggling={toggling[channel.channel_id] === true}
            ontest={() => void test(channel)}
            onedit={() => openEdit(channel)}
            ondelete={() => (deleting = channel)}
            ontoggle={(on) => void toggle(channel, on)}
          />
        {/each}
      </ul>
    {/if}
  </SettingsSection>

  <NotifiedEvents events={notified} saving={savingEvent} ontoggle={(e) => void toggleEvent(e)} />

  <RecentSends {sends} failed={sendsFailed} {channels} />
</SettingsSubPage>

<ChannelDialog
  open={dialogOpen}
  channel={editing}
  notified={notified ?? []}
  onclose={() => (dialogOpen = false)}
  onsaved={onSaved}
/>

<ConfirmDialog
  open={deleting !== null}
  onclose={() => (deleting = null)}
  onconfirm={confirmDelete}
  title={$t("notifications.delete_confirm_title")}
  message={$t("settings.notifications.delete_message", { values: { name: deleting ? channelName(deleting) : "" } })}
  confirmLabel={$t("notifications.delete_confirm_yes")}
  cancelLabel={$t("common.cancel")}
  loading={deleteBusy}
  data-testid="delete-channel-confirm"
/>
