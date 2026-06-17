<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type {
    ChannelTestResult,
    NotificationChannel,
    NotificationChannelView,
    UpdateChannelRequest,
  } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { addToast } from "$lib/components/ui/toast/store";
  import { eventLabelKey } from "$lib/notifications/event-labels";
  import { Monitor, Webhook, Pencil, Trash2, Send } from "lucide-svelte";
  import { EntityCard } from "$lib/components/operator";

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

  /**
   * Per-type presentation : icon + tint class for the icon container, and the
   * canonical Badge variant. We use the design system's variants directly
   * (info / primary) rather than overriding text/border via extraClass - that
   * was the source of the contrast issues on the previous version.
   */
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

  async function handleToggle(next: boolean) {
    if (toggling) return;
    toggling = true;
    const previous = channel.enabled;
    channel.enabled = next; // optimistic flip
    try {
      const request: UpdateChannelRequest = { enabled: next };
      await invoke<NotificationChannelView>("update_notification_channel", {
        id: channel.channel_id,
        channel: request,
      });
      addToast(
        $t(next ? "notifications.toggle_enabled_toast" : "notifications.toggle_disabled_toast", {
          values: { label: displayName },
        }),
        "success",
      );
      ontoggled?.(channel.channel_id, next);
    } catch (err: unknown) {
      channel.enabled = previous;
      const message = err instanceof Error ? err.message : String(err);
      addToast($t("notifications.toggle_error_toast", { values: { message } }), "error");
    } finally {
      toggling = false;
    }
  }

  async function handleTest() {
    testing = true;
    testResult = null;
    if (feedbackTimer !== null) {
      clearTimeout(feedbackTimer);
      feedbackTimer = null;
    }
    try {
      const result: ChannelTestResult = await invoke("test_notification_channel", {
        channelId: channel.channel_id,
      });
      testResult = result;
      if (result.status === "ok") {
        addToast($t("notifications.test_success"), "success");
      } else {
        addToast(result.error ?? $t("common.status.error"), "error");
      }
    } catch (err: unknown) {
      testResult = {
        channel_id: channel.channel_id,
        status: "error",
        error: err instanceof Error ? err.message : String(err),
        latency_ms: null,
      };
      addToast(testResult.error ?? $t("common.status.error"), "error");
    } finally {
      testing = false;
      feedbackTimer = setTimeout(() => {
        testResult = null;
        feedbackTimer = null;
      }, FEEDBACK_DURATION_MS);
    }
  }
</script>

<EntityCard
  accent={typeStyle.tone}
  iconTone={typeStyle.tone}
  title={displayName}
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
      <p class="-mt-1 truncate font-mono text-[10px] text-muted-foreground/60" title={channel.channel_id}>
        {channel.channel_id}
      </p>
    {/if}
    <!-- Event pills + throttle summary -->
    <div class="flex flex-wrap items-center gap-1">
      {#if channel.events.length > 0}
        {#each channel.events as event}
          <span
            class="rounded-full bg-primary/10 px-2 py-px text-[10px] text-primary/80"
            title={event}
          >
            {eventDisplay(event)}
          </span>
        {/each}
      {:else}
        <span class="text-[11px] text-muted-foreground/70">
          {$t('notifications.all_events')}
        </span>
      {/if}
      {#if throttleSummary(channel.min_interval_seconds)}
        <span class="ml-auto text-[10px] text-muted-foreground/60">
          ⏱ {throttleSummary(channel.min_interval_seconds)}
        </span>
      {/if}
    </div>
  {/snippet}

  {#snippet actions()}
    <div class="min-h-[20px]">
      {#if testResult}
        {#if testResult.status === "ok"}
          <Badge variant="success" size="sm">
            OK{testResult.latency_ms !== null ? ` · ${testResult.latency_ms} ms` : ""}
          </Badge>
        {:else}
          <Badge variant="danger" size="sm" class="max-w-[220px]">
            <span class="truncate">{$t('common.status.error')}{testResult.error ? `: ${testResult.error}` : ""}</span>
          </Badge>
        {/if}
      {/if}
    </div>
    <div class="ml-auto flex items-center gap-1 shrink-0">
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
