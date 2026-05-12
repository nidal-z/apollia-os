<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { NotificationChannel, ChannelTestResult } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";

  interface Props {
    channel: NotificationChannel;
    onedit: () => void;
    ondelete: () => void;
  }

  let { channel, onedit, ondelete }: Props = $props();

  const FEEDBACK_DURATION_MS = 5_000;

  let testing = $state(false);
  let testResult = $state<ChannelTestResult | null>(null);
  let feedbackTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  const TYPE_BADGE: Record<string, { label: string; extraClass: string }> = {
    desktop: { label: "Desktop", extraClass: "border-info text-info" },
    webhook: { label: "Webhook", extraClass: "border-accent text-accent-foreground" },
    sse: { label: "SSE", extraClass: "border-info text-info-foreground" },
  };

  const badgeConfig = $derived(
    TYPE_BADGE[channel.type] ?? {
      label: channel.type.toUpperCase(),
      extraClass: "",
    },
  );

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

<div class="glass-card-hover glass-border rounded-lg relative overflow-hidden" data-testid="channel-card-{channel.channel_id}">
  <!-- Accent bar left -->
  <div class="absolute left-0 top-0 bottom-0 w-1 {channel.enabled ? 'bg-primary' : 'bg-muted-foreground/20'}" />

  <div class="pl-4 pr-3.5 pt-3 pb-2.5">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2 min-w-0">
        <div class="min-w-0">
          <h3 class="text-[13px] font-medium truncate">
            {channel.label && channel.label.trim() ? channel.label : channel.channel_id}
          </h3>
          {#if channel.label && channel.label.trim()}
            <p class="text-[10px] text-muted-foreground/70 font-mono truncate" title={channel.channel_id}>
              {channel.channel_id}
            </p>
          {/if}
        </div>
        <Badge variant="outline" class={badgeConfig.extraClass}>
          {badgeConfig.label}
        </Badge>
      </div>
      <div class="flex items-center gap-1.5">
        {#if channel.enabled}
          <Badge variant="success">{$t('notifications.enabled')}</Badge>
        {:else}
          <Badge variant="secondary">{$t('notifications.disabled')}</Badge>
        {/if}
      </div>
    </div>

    <!-- Event filters -->
    {#if channel.events.length > 0}
      <div class="mt-2 flex flex-wrap gap-1">
        {#each channel.events as event}
          <Badge variant="outline" class="text-[11px]">
            {event}
          </Badge>
        {/each}
      </div>
    {:else}
      <p class="mt-1 text-[11px] text-muted-foreground">{$t('notifications.all_events')}</p>
    {/if}

    <!-- Actions row -->
    <div class="mt-3 flex items-center gap-2">
      <Button
        size="sm"
        variant="outline"
        onclick={handleTest}
        disabled={testing || !channel.enabled}
        data-testid="channel-test-btn-{channel.channel_id}"
      >
        {testing ? $t('notifications.testing') : $t('notifications.test')}
      </Button>

      <Button
        size="sm"
        variant="outline"
        onclick={onedit}
        data-testid="channel-edit-btn-{channel.channel_id}"
      >
        {$t('notifications.edit')}
      </Button>

      <Button
        size="sm"
        variant="outline"
        class="text-destructive hover:bg-destructive/10"
        onclick={ondelete}
        data-testid="channel-delete-btn-{channel.channel_id}"
      >
        {$t('notifications.delete')}
      </Button>

      {#if testResult}
        {#if testResult.status === "ok"}
          <Badge variant="success">
            OK{testResult.latency_ms !== null ? ` (${testResult.latency_ms}ms)` : ""}
          </Badge>
        {:else}
          <Badge variant="destructive">
            {$t('common.status.error')}{testResult.error ? `: ${testResult.error}` : ""}
          </Badge>
        {/if}
      {/if}
    </div>
  </div>
</div>
