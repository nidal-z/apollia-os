<script lang="ts">
  import { t } from "svelte-i18n";
  import type {
    ChannelTestResult,
    NotificationChannel,
    UpdateChannelRequest,
  } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import { eventLabelKey } from "$lib/notifications/event-labels";
  import { Monitor, Webhook, Pencil, Trash2, Send, Power } from "lucide-svelte";
  import { EntityCard } from "$lib/components/operator";
  import { ContextMenu } from "$lib/components/ui/context-menu";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";
  import { updateNotificationChannel, testNotificationChannel } from "$lib/ipc/notifications";

  interface Props {
    channel: NotificationChannel;
    onedit: () => void;
    ondelete: () => void;
    /** Bubble up the new `enabled` state so the parent can refresh its store. */
    ontoggled?: (id: string, enabled: boolean) => void;
  }

  let { channel, onedit, ondelete, ontoggled }: Props = $props();

  const FEEDBACK_DURATION_MS = 5_000;

  let testing = $state(false);
  let toggling = $state(false);
  let testResult = $state<ChannelTestResult | null>(null);
  let feedbackTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  /** Per-type presentation: icon, accent tone and canonical Badge variant. */
  type Tone = "primary" | "info";
  type ChannelTypeStyle = {
    Icon: typeof Monitor;
    tone: Tone;
    badgeVariant: "info" | "primary";
    labelKey: string;
  };
  const TYPE_STYLES: Record<string, ChannelTypeStyle> = {
    desktop: {
      Icon: Monitor,
      tone: "info",
      badgeVariant: "info",
      labelKey: "notifications.field_type_desktop",
    },
    webhook: {
      Icon: Webhook,
      tone: "primary",
      badgeVariant: "primary",
      labelKey: "notifications.field_type_webhook",
    },
  };
  const typeStyle = $derived(
    TYPE_STYLES[channel.type] ?? {
      Icon: Monitor,
      tone: "primary" as const,
      badgeVariant: "primary" as const,
      labelKey: "",
    },
  );

  const displayName = $derived(
    channel.label && channel.label.trim() ? channel.label : channel.channel_id,
  );
  const hasCustomLabel = $derived(!!(channel.label && channel.label.trim()));

  /** Humanize an event id via i18n, falling back to the raw id when no key matches. */
  function eventDisplay(event: string): string {
    const key = eventLabelKey(event);
    const translated = $t(key);
    return translated === key ? event : translated;
  }

  /** Throttle presets get a short human label; custom values show the raw seconds. */
  function throttleSummary(seconds: number | undefined): string | null {
    if (!seconds || seconds <= 0) return null;
    if (seconds === 60) return $t("notifications.throttle_options.minute");
    if (seconds === 300) return $t("notifications.throttle_options.five_min");
    if (seconds === 3600) return $t("notifications.throttle_options.hour");
    return `${seconds} s`;
  }

  /** Inline failure feedback, humanized. Raw text stays on the badge `title`. */
  const testFailure = $derived.by(() => {
    if (!testResult || testResult.status === "ok") return null;
    const raw = testResult.error ?? $t("common.status.error");
    const humanized = reportError(raw, { surface: "inline" });
    return { message: humanized.title, detail: raw };
  });

  async function handleToggle(next: boolean) {
    if (toggling) return;
    toggling = true;
    const previous = channel.enabled;
    channel.enabled = next; // optimistic flip
    try {
      const request: UpdateChannelRequest = { enabled: next };
      await updateNotificationChannel(channel.channel_id, request);
      addToast(
        $t(next ? "notifications.toggle_enabled_toast" : "notifications.toggle_disabled_toast", {
          values: { label: displayName },
        }),
        "success",
      );
      ontoggled?.(channel.channel_id, next);
    } catch (err: unknown) {
      channel.enabled = previous;
      reportError(err, { surface: "toast" });
    } finally {
      toggling = false;
    }
  }

  async function handleTest() {
    if (testing || !channel.enabled) return;
    testing = true;
    testResult = null;
    if (feedbackTimer !== null) {
      clearTimeout(feedbackTimer);
      feedbackTimer = null;
    }
    try {
      const result = await testNotificationChannel(channel.channel_id);
      testResult = result;
      if (result.status === "ok") {
        addToast($t("notifications.test_success"), "success");
      } else {
        reportError(result.error ?? $t("common.status.error"), { surface: "toast" });
      }
    } catch (err: unknown) {
      testResult = {
        channel_id: channel.channel_id,
        status: "error",
        error: err instanceof Error ? err.message : String(err),
        latency_ms: null,
      };
      reportError(err, { surface: "toast" });
    } finally {
      testing = false;
      feedbackTimer = setTimeout(() => {
        testResult = null;
        feedbackTimer = null;
      }, FEEDBACK_DURATION_MS);
    }
  }

  /** Right-click actions, mirrored from the footer icon cluster. */
  const menuItems = $derived<ActionMenuItem[]>([
    {
      id: "test",
      label: $t("notifications.test"),
      icon: Send,
      onclick: handleTest,
      disabled: testing || !channel.enabled,
      testid: `channel-ctx-test-${channel.channel_id}`,
    },
    {
      id: "edit",
      label: $t("notifications.edit"),
      icon: Pencil,
      onclick: onedit,
      testid: `channel-ctx-edit-${channel.channel_id}`,
    },
    {
      id: "toggle",
      label: $t("notifications.toggle_action"),
      icon: Power,
      onclick: () => handleToggle(!channel.enabled),
      disabled: toggling,
      testid: `channel-ctx-toggle-${channel.channel_id}`,
    },
    {
      id: "delete",
      label: $t("notifications.delete"),
      icon: Trash2,
      onclick: ondelete,
      variant: "destructive",
      testid: `channel-ctx-delete-${channel.channel_id}`,
    },
  ]);
</script>

<ContextMenu items={menuItems} data-testid="channel-ctx-menu-{channel.channel_id}">
  <EntityCard
    accent={typeStyle.tone}
    iconTone={typeStyle.tone}
    title={displayName}
    class="hover-lift {channel.enabled ? '' : 'opacity-60'}"
    data-testid="channel-card-{channel.channel_id}"
  >
    {#snippet icon()}
      <typeStyle.Icon size={16} />
    {/snippet}

    {#snippet badges()}
      {#if typeStyle.labelKey}
        <Badge variant={typeStyle.badgeVariant} size="sm" class="shrink-0">
          {$t(typeStyle.labelKey)}
        </Badge>
      {/if}
    {/snippet}

    {#snippet trailing()}
      <Toggle
        checked={channel.enabled}
        onchange={handleToggle}
        loading={toggling}
        size="sm"
        aria-label={$t(channel.enabled ? "notifications.disabled" : "notifications.enabled")}
        data-testid="channel-toggle-{channel.channel_id}"
      />
    {/snippet}

    {#snippet body()}
      {#if hasCustomLabel}
        <p class="-mt-1 truncate font-mono text-caption text-muted-foreground/60" title={channel.channel_id}>
          {channel.channel_id}
        </p>
      {/if}
      <!-- Event pills + throttle summary -->
      <div class="flex flex-wrap items-center gap-1">
        {#if channel.events.length > 0}
          {#each channel.events as event (event)}
            <span
              class="rounded-full bg-primary/10 px-2 py-px text-caption text-primary/80"
              title={event}
            >
              {eventDisplay(event)}
            </span>
          {/each}
        {:else}
          <span class="text-caption text-muted-foreground/70">
            {$t('notifications.all_events')}
          </span>
        {/if}
        {#if throttleSummary(channel.min_interval_seconds)}
          <span class="ml-auto text-caption text-muted-foreground/60">
            &#9201; {throttleSummary(channel.min_interval_seconds)}
          </span>
        {/if}
      </div>
    {/snippet}

    {#snippet actions()}
      <div class="min-h-5 flex-1">
        {#if testResult?.status === "ok"}
          <Badge variant="success" size="sm">
            OK{testResult.latency_ms !== null ? ` · ${testResult.latency_ms} ms` : ""}
          </Badge>
        {:else if testFailure}
          <Badge variant="danger" size="sm" class="max-w-56" title={testFailure.detail}>
            <span class="truncate">{testFailure.message}</span>
          </Badge>
        {/if}
      </div>
      <div class="ml-auto flex shrink-0 items-center gap-1">
        <Button
          size="icon"
          variant="ghost"
          class="size-7"
          onclick={handleTest}
          disabled={testing || !channel.enabled}
          title={$t('notifications.test')}
          data-testid="channel-test-btn-{channel.channel_id}"
        >
          <Send size={13} class={testing ? 'animate-pulse' : ''} />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          class="size-7"
          onclick={onedit}
          title={$t('notifications.edit')}
          data-testid="channel-edit-btn-{channel.channel_id}"
        >
          <Pencil size={13} />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          class="size-7 text-destructive/60 hover:text-destructive"
          onclick={ondelete}
          title={$t('notifications.delete')}
          data-testid="channel-delete-btn-{channel.channel_id}"
        >
          <Trash2 size={13} />
        </Button>
      </div>
    {/snippet}
  </EntityCard>
</ContextMenu>
