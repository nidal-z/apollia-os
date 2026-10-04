<script lang="ts">
  /**
   * One notification channel in the Notifications section: the glyph of its
   * type, its name, a detail line (where, which events, how often), the
   * outcome of its last test, then Test, a menu holding Edit and Delete, and
   * the switch that turns it on or off. A channel switched off cannot be
   * tested.
   */
  import { t } from "svelte-i18n";
  import { Monitor, Globe, Pencil, Trash2, MoreHorizontal, Check, AlertTriangle } from "lucide-svelte";
  import type { ChannelTestResult, NotificationChannel } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Spinner } from "$lib/components/ui/progress";
  import { ActionMenu, type ActionMenuItem } from "$lib/components/ui/action-menu";
  import { humanize } from "$lib/errors/humanize";
  import { channelDetailKeys, channelName } from "$lib/notifications/notificationsSection";

  interface Props {
    channel: NotificationChannel;
    /** `true` while a test is on its way. */
    testing: boolean;
    /** Outcome of the last test of this channel, if any. */
    result: ChannelTestResult | null;
    /** `true` while the switch is being saved. */
    toggling: boolean;
    ontest: () => void;
    onedit: () => void;
    ondelete: () => void;
    ontoggle: (enabled: boolean) => void;
  }

  let { channel, testing, result, toggling, ontest, onedit, ondelete, ontoggle }: Props = $props();

  const name = $derived(channelName(channel));
  const detail = $derived.by(() => {
    const keys = channelDetailKeys(channel);
    const events = keys.eventsKey
      ? $t(keys.eventsKey)
      : $t("settings.notifications.row.event_count", { values: { count: keys.eventCount } });
    const limit = $t(keys.throttleKey, { values: { seconds: keys.throttleSeconds } });
    return [$t(keys.typeKey), events, limit].join(" · ");
  });
  const failure = $derived(
    result && result.status !== "ok"
      ? humanize(result.error ?? result.status, (k) => $t(k)).title
      : null,
  );

  const menuItems = $derived<ActionMenuItem[]>([
    {
      id: "edit",
      label: $t("notifications.edit"),
      icon: Pencil,
      onclick: onedit,
      testid: `channel-edit-btn-${channel.channel_id}`,
    },
    {
      id: "delete",
      label: $t("notifications.delete"),
      icon: Trash2,
      onclick: ondelete,
      variant: "destructive",
      testid: `channel-delete-btn-${channel.channel_id}`,
    },
  ]);
</script>

<li
  class="flex items-start gap-3 border-b border-border px-4 py-3 last:border-b-0"
  data-testid="channel-card-{channel.channel_id}"
>
  <span
    class="mt-0.5 flex size-8 flex-none items-center justify-center rounded-md bg-muted text-muted-foreground"
    aria-hidden="true"
  >
    {#if channel.type === "webhook"}
      <Globe size={15} strokeWidth={1.75} />
    {:else}
      <Monitor size={15} strokeWidth={1.75} />
    {/if}
  </span>

  <div class="min-w-0 flex-1 {channel.enabled ? '' : 'opacity-60'}">
    <p class="truncate text-body-sm font-medium text-foreground" title={channel.channel_id}>{name}</p>
    <p class="truncate text-caption text-muted-foreground">{detail}</p>
    {#if testing}
      <p class="mt-1 flex items-center gap-1.5 text-caption text-muted-foreground" data-testid="channel-test-pending-{channel.channel_id}">
        <Spinner size={12} />
        {$t("settings.notifications.row.test_pending")}
      </p>
    {:else if result?.status === "ok"}
      <p class="mt-1 flex items-center gap-1.5 text-caption text-success-a11y" data-testid="channel-test-ok-{channel.channel_id}">
        <Check size={12} strokeWidth={2} aria-hidden="true" />
        {result.latency_ms !== null
          ? $t("settings.notifications.row.test_ok_latency", { values: { ms: result.latency_ms } })
          : $t("settings.notifications.row.test_ok")}
      </p>
    {:else if failure}
      <p
        class="mt-1 flex items-center gap-1.5 text-caption text-destructive"
        title={result?.error ?? ""}
        data-testid="channel-test-failed-{channel.channel_id}"
      >
        <AlertTriangle size={12} strokeWidth={2} aria-hidden="true" />
        {$t("settings.notifications.row.test_failed", { values: { reason: failure } })}
      </p>
    {/if}
  </div>

  <div class="flex flex-none items-center gap-1.5">
    <Button
      variant="outline"
      size="sm"
      onclick={ontest}
      disabled={testing || !channel.enabled}
      data-testid="channel-test-btn-{channel.channel_id}"
    >
      {$t("notifications.test")}
    </Button>
    <ActionMenu
      items={menuItems}
      align="end"
      side="bottom"
      class="w-40"
      triggerLabel={$t("settings.notifications.row.more_actions", { values: { name } })}
      data-testid="channel-menu-{channel.channel_id}"
    >
      {#snippet triggerSlot(props)}
        <button
          type="button"
          {...props}
          class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          aria-label={$t("settings.notifications.row.more_actions", { values: { name } })}
          data-testid="channel-menu-{channel.channel_id}"
        >
          <MoreHorizontal size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
      {/snippet}
    </ActionMenu>
    <Toggle
      checked={channel.enabled}
      onchange={ontoggle}
      loading={toggling}
      size="sm"
      aria-label={$t("settings.notifications.row.enabled_label", { values: { name } })}
      data-testid="channel-toggle-{channel.channel_id}"
    />
  </div>
</li>
