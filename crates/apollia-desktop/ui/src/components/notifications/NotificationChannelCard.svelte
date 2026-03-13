<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { NotificationChannel, ChannelTestResult } from "$lib/types";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    channel: NotificationChannel;
  }

  let { channel }: Props = $props();

  const FEEDBACK_DURATION_MS = 5_000;

  let testing = $state(false);
  let testResult = $state<ChannelTestResult | null>(null);
  let feedbackTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  const TYPE_BADGE: Record<string, { label: string; extraClass: string }> = {
    desktop: { label: "Desktop", extraClass: "border-blue-500 text-blue-500" },
    webhook: { label: "Webhook", extraClass: "border-purple-500 text-purple-500" },
    sse: { label: "SSE", extraClass: "border-cyan-500 text-cyan-500" },
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
    } catch (err: unknown) {
      testResult = {
        channel_id: channel.channel_id,
        status: "error",
        error: err instanceof Error ? err.message : String(err),
        latency_ms: null,
      };
    } finally {
      testing = false;
      feedbackTimer = setTimeout(() => {
        testResult = null;
        feedbackTimer = null;
      }, FEEDBACK_DURATION_MS);
    }
  }
</script>

<Card class="relative overflow-hidden">
  <CardHeader class="pb-2">
    <div class="flex items-center justify-between">
      <CardTitle class="text-base font-semibold">{channel.channel_id}</CardTitle>
      <div class="flex items-center gap-2">
        <Badge variant="outline" class={badgeConfig.extraClass}>
          {badgeConfig.label}
        </Badge>
        {#if channel.enabled}
          <Badge variant="default" class="bg-[var(--apollia-success)] text-white">
            Enabled
          </Badge>
        {:else}
          <Badge variant="secondary">Disabled</Badge>
        {/if}
      </div>
    </div>
  </CardHeader>

  <CardContent>
    <div class="space-y-3">
      <!-- Event filters -->
      {#if channel.events.length > 0}
        <div class="flex flex-wrap gap-1">
          {#each channel.events as event}
            <Badge variant="outline" class="text-xs">
              {event}
            </Badge>
          {/each}
        </div>
      {:else}
        <p class="text-xs text-muted-foreground">All events</p>
      {/if}

      <!-- Test button + feedback -->
      <div class="flex items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          onclick={handleTest}
          disabled={testing || !channel.enabled}
        >
          {testing ? "Testing..." : "Tester"}
        </Button>

        {#if testResult}
          {#if testResult.status === "ok"}
            <Badge
              variant="default"
              class="bg-[var(--apollia-success)] text-white"
            >
              OK{testResult.latency_ms !== null ? ` (${testResult.latency_ms}ms)` : ""}
            </Badge>
          {:else}
            <Badge variant="destructive">
              Erreur{testResult.error ? `: ${testResult.error}` : ""}
            </Badge>
          {/if}
        {/if}
      </div>
    </div>
  </CardContent>
</Card>
