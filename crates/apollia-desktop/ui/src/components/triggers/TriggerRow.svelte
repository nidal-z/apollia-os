<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { TriggerStatus, TriggerFireResult } from "$lib/types";
  import { humanizeTrigger } from "$lib/utils/humanize-trigger";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { addToast } from "$lib/components/ui/toast/store";

  interface Props {
    trigger: TriggerStatus;
    locale: string;
    isBuilder: boolean;
    onfire: (taskId: string) => void;
    onlogs: (triggerId: string) => void;
    onedit: (triggerId: string) => void;
    ondelete: (triggerId: string) => void;
  }

  let { trigger, locale, isBuilder, onfire, onlogs, onedit, ondelete }: Props = $props();

  let toggling = $state(false);
  let firing = $state(false);

  const SOURCE_BADGE: Record<
    TriggerStatus["source_kind"],
    { label: string; extraClass: string }
  > = {
    cron: { label: "CRON", extraClass: "border-info text-info" },
    interval: {
      label: "INTERVAL",
      extraClass: "border-info text-info-foreground",
    },
    file_watch: {
      label: "FILE",
      extraClass: "border-success text-success",
    },
    webhook: {
      label: "WEBHOOK",
      extraClass: "border-accent text-accent-foreground",
    },
    oneshot: {
      label: "ONESHOT",
      extraClass: "border-warning text-warning",
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
    try {
      await invoke("set_trigger_enabled", {
        id: trigger.id,
        enabled: !trigger.enabled,
      });
      addToast(
        $t(trigger.enabled ? "triggers.disabled_toast" : "triggers.enabled_toast", { values: { id: trigger.id } }),
        "success",
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      addToast(msg, "error");
    } finally {
      toggling = false;
    }
  }

  async function handleFire() {
    firing = true;
    try {
      const result: TriggerFireResult = await invoke("fire_trigger", {
        id: trigger.id,
      });
      onfire(result.task_id);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      addToast(msg, "error");
    } finally {
      firing = false;
    }
  }
</script>

<div class="glass-card-hover relative overflow-hidden" data-testid="trigger-row-{trigger.id}">
  <!-- Status accent bar -->
  <div
    class="h-0.5 w-full {trigger.enabled ? 'bg-primary' : 'bg-muted-foreground/20'}"
    data-testid="trigger-status-bar"
  ></div>

  <div class="px-3.5 pt-3 pb-2.5">
    <!-- Header: name + badge + toggle -->
    <div class="flex items-center justify-between">
      <div class="flex min-w-0 flex-1 items-center gap-2">
        <h3 class="truncate text-[13px] font-medium">{trigger.id}</h3>
        <Badge variant="outline" class={badgeConfig.extraClass}>
          {badgeConfig.label}
        </Badge>
      </div>
      <Toggle
        checked={trigger.enabled}
        onchange={handleToggle}
        disabled={toggling}
        data-testid="trigger-toggle-{trigger.id}"
      />
    </div>

    <!-- Description -->
    <p class="mt-1 text-[11px] text-muted-foreground" data-testid="trigger-description-{trigger.id}">
      {humanDescription}
    </p>

    <!-- Raw config (builder only) -->
    {#if isBuilder && trigger.source_config}
      <p class="mt-0.5 text-[11px] text-muted-foreground/60 font-mono" data-testid="trigger-raw-{trigger.id}">
        {trigger.source_config}
      </p>
    {/if}

    <!-- Stats row -->
    <div class="mt-2 flex items-center gap-4 text-[11px] text-muted-foreground">
      <span>{$t('triggers.fires')}: <span class="font-medium text-foreground">{trigger.fire_count}</span></span>
      <span>{$t('triggers.skips')}: <span class="font-medium text-foreground">{trigger.skip_count}</span></span>
      {#if trigger.last_fired}
        <span>{$t('triggers.last_fire')}: <span class="font-medium text-foreground">{formatRelativeTime(trigger.last_fired)}</span></span>
      {/if}
    </div>

    <!-- Actions -->
    <div class="mt-3 flex gap-2">
      <Button
        size="sm"
        variant="primary-solid"
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
      {#if isBuilder}
        <Button
          size="sm"
          variant="ghost"
          onclick={() => onedit(trigger.id)}
          data-testid="trigger-edit-btn"
        >
          {$t('triggers.edit')}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          class="text-destructive"
          onclick={() => ondelete(trigger.id)}
          data-testid="trigger-delete-btn"
        >
          {$t('triggers.delete')}
        </Button>
      {/if}
    </div>
  </div>
</div>
