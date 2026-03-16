<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { TriggerStatus, TriggerFireResult } from "$lib/types";
  import { humanizeTrigger } from "$lib/utils/humanize-trigger";
  import { Card, CardContent } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    trigger: TriggerStatus;
    locale: string;
    isBuilder: boolean;
    onfire: (taskId: string) => void;
    onlogs: (triggerId: string) => void;
  }

  let { trigger, locale, isBuilder, onfire, onlogs }: Props = $props();

  let toggling = $state(false);
  let firing = $state(false);
  let toggleError = $state<string | null>(null);
  let fireError = $state<string | null>(null);

  const SOURCE_BADGE: Record<
    TriggerStatus["source_kind"],
    { label: string; extraClass: string }
  > = {
    cron: { label: "CRON", extraClass: "border-blue-500 text-blue-500" },
    interval: {
      label: "INTERVAL",
      extraClass: "border-cyan-500 text-cyan-500",
    },
    file_watch: {
      label: "FILE",
      extraClass: "border-green-500 text-green-500",
    },
    webhook: {
      label: "WEBHOOK",
      extraClass: "border-purple-500 text-purple-500",
    },
    oneshot: {
      label: "ONESHOT",
      extraClass: "border-orange-500 text-orange-500",
    },
  };

  const badgeConfig = $derived(
    SOURCE_BADGE[trigger.source_kind] ?? {
      label: trigger.source_kind.toUpperCase(),
      extraClass: "",
    },
  );

  const humanDescription = $derived(
    humanizeTrigger(trigger.source_kind, trigger.source_config, locale),
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

  async function handleToggle() {
    toggling = true;
    toggleError = null;
    try {
      await invoke("set_trigger_enabled", {
        id: trigger.id,
        enabled: !trigger.enabled,
      });
    } catch (err: unknown) {
      toggleError = err instanceof Error ? err.message : String(err);
    } finally {
      toggling = false;
    }
  }

  async function handleFire() {
    firing = true;
    fireError = null;
    try {
      const result: TriggerFireResult = await invoke("fire_trigger", {
        id: trigger.id,
      });
      onfire(result.task_id);
    } catch (err: unknown) {
      fireError = err instanceof Error ? err.message : String(err);
    } finally {
      firing = false;
    }
  }
</script>

<Card class="relative overflow-hidden" data-testid="trigger-row-{trigger.id}">
  <CardContent class="py-3">
    <div class="flex items-center gap-4">
      <!-- Trigger description and source badge -->
      <div class="flex min-w-0 flex-1 items-center gap-3">
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <span class="text-sm font-medium" data-testid="trigger-description-{trigger.id}">
              {humanDescription}
            </span>
            <Badge variant="outline" class={badgeConfig.extraClass}>
              {badgeConfig.label}
            </Badge>
          </div>
          {#if isBuilder && trigger.source_config}
            <div class="mt-0.5 text-xs text-muted-foreground" data-testid="trigger-raw-{trigger.id}">
              {trigger.source_config}
            </div>
          {/if}
        </div>
      </div>

      <!-- Stats -->
      <div class="flex items-center gap-4 text-xs text-muted-foreground">
        <div class="text-center">
          <div class="font-medium text-foreground">{trigger.fire_count}</div>
          <div>{$t('triggers.fires')}</div>
        </div>
        <div class="text-center">
          <div class="font-medium text-foreground">{trigger.skip_count}</div>
          <div>{$t('triggers.skips')}</div>
        </div>
        {#if trigger.last_fired}
          <div class="min-w-[60px] text-center">
            <div class="font-medium text-foreground">
              {formatRelativeTime(trigger.last_fired)}
            </div>
            <div>{$t('triggers.last_fire')}</div>
          </div>
        {/if}
      </div>

      <!-- Toggle -->
      <button
        class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors {trigger.enabled
          ? 'bg-[var(--apollia-success)]'
          : 'bg-muted'}"
        onclick={handleToggle}
        disabled={toggling}
        aria-label={trigger.enabled ? $t('triggers.disable_trigger') : $t('triggers.enable_trigger')}
        data-testid="trigger-toggle-{trigger.id}"
      >
        <span
          class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow transition-transform {trigger.enabled
            ? 'translate-x-4'
            : 'translate-x-0.5'}"
        ></span>
      </button>

      <!-- Actions -->
      <div class="flex items-center gap-1">
        <Button
          size="sm"
          variant="outline"
          onclick={handleFire}
          disabled={firing || !trigger.enabled}
          data-testid="trigger-fire-{trigger.id}"
        >
          {firing ? "..." : $t('triggers.fire')}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onclick={() => onlogs(trigger.id)}
          data-testid="trigger-logs-{trigger.id}"
        >
          {$t('triggers.logs')}
        </Button>
      </div>
    </div>

    <!-- Error display -->
    {#if toggleError}
      <div
        class="mt-2 rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-3 py-1 text-xs text-[hsl(var(--destructive))]"
      >
        {toggleError}
      </div>
    {/if}
    {#if fireError}
      <div
        class="mt-2 rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-3 py-1 text-xs text-[hsl(var(--destructive))]"
      >
        {fireError}
      </div>
    {/if}
  </CardContent>
</Card>
