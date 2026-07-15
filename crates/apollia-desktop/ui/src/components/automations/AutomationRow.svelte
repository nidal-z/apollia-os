<script lang="ts">
  /**
   * Dense row for the operator Automations table.
   *
   * Matches the visual pattern of `TaskRow` / `MemoryRow` - single line with
   * column-aligned cells and a hover-revealed action menu. No glass-card,
   * no gradient bars - only V3 surface tokens.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Play, Pause, History, Trash2, MoreHorizontal, Calendar, Clock, FolderSync, Webhook, Zap } from "lucide-svelte";
  import type { TriggerStatus, TriggerFireResult } from "$lib/types";
  import { StatusDot } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Avatar } from "$lib/components/ui/avatar";
  import { addToast } from "$lib/components/ui/toast/store";
  import { refreshTriggers } from "$lib/stores/sse";
  import { ActionMenu, type ActionMenuItem } from "$lib/components/ui/action-menu";
  import {
    estimateNextRun,
    formatNextRun,
    humanizeSchedule,
  } from "$lib/automations/humanize";

  interface Props {
    trigger: TriggerStatus;
    locale: string;
    lastError?: { message: string; firedAt: string } | null;
    onfire: (taskId: string) => void;
    onlogs: (triggerId: string) => void;
    ondelete: (triggerId: string) => void;
  }

  let {
    trigger,
    locale,
    lastError = null,
    onfire,
    onlogs,
    ondelete,
  }: Props = $props();

  let firing = $state(false);

  type Status = "active" | "paused" | "error";
  const status = $derived<Status>(
    !trigger.enabled ? "paused" : lastError ? "error" : "active",
  );

  const STATUS_TONE: Record<Status, "success" | "neutral" | "danger"> = {
    active: "success",
    paused: "neutral",
    error: "danger",
  };
  const STATUS_DOT: Record<Status, string> = {
    active: "hsl(var(--success))",
    paused: "hsl(var(--muted-foreground))",
    error: "hsl(var(--destructive))",
  };

  const ScheduleIcon = $derived(
    trigger.source_kind === "cron" ? Calendar :
    trigger.source_kind === "interval" ? Clock :
    trigger.source_kind === "file_watch" ? FolderSync :
    trigger.source_kind === "webhook" ? Webhook :
    Zap,
  );

  const humanized = $derived(
    humanizeSchedule(trigger.source_kind, trigger.source_config, locale),
  );

  const lastFiredDate = $derived(trigger.last_fired ? new Date(trigger.last_fired) : null);
  const nextRun = $derived(
    trigger.enabled
      ? estimateNextRun(trigger.source_kind, trigger.source_config, lastFiredDate)
      : null,
  );
  const nextRunLabel = $derived(
    trigger.enabled ? formatNextRun(nextRun, locale) : $t("automations.paused_hint"),
  );

  function formatRelative(iso: string): string {
    const diffSec = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 1000));
    if (diffSec < 60) return $t("automations.relative.seconds", { values: { n: diffSec } });
    const minutes = Math.floor(diffSec / 60);
    if (minutes < 60) return $t("automations.relative.minutes", { values: { n: minutes } });
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return $t("automations.relative.hours", { values: { n: hours } });
    const days = Math.floor(hours / 24);
    return $t("automations.relative.days", { values: { n: days } });
  }

  const lastRunLabel = $derived(
    trigger.last_fired ? formatRelative(trigger.last_fired) : "-",
  );

  const humanTitle = $derived(
    trigger.id.replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase()),
  );

  const statusLabel = $derived(
    status === "active"
      ? $t("automations.status.active")
      : status === "error"
        ? $t("automations.status.error")
        : $t("automations.status.paused"),
  );

  async function handleFire(e: MouseEvent) {
    e.stopPropagation();
    if (firing || !trigger.enabled) return;
    firing = true;
    try {
      const result: TriggerFireResult = await invoke("fire_trigger", { id: trigger.id });
      onfire(result.task_id);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      addToast(msg, "error");
    } finally {
      firing = false;
    }
  }

  let toggling = $state(false);

  // Enable/disable the trigger. Reflects the AUTO-9 pause/resume gap: the
  // backend already exposes set_trigger_enabled, this surfaces it per-row.
  async function handleToggleEnabled() {
    if (toggling) return;
    toggling = true;
    const next = !trigger.enabled;
    try {
      await invoke("set_trigger_enabled", { id: trigger.id, enabled: next });
      addToast(
        next ? $t("automations.resume_toast") : $t("automations.pause_toast"),
        "success",
      );
      await refreshTriggers();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      toggling = false;
    }
  }

  const menuItems = $derived<ActionMenuItem[]>([
    {
      id: "toggle-enabled",
      label: trigger.enabled ? $t("automations.pause") : $t("automations.resume"),
      icon: trigger.enabled ? Pause : Play,
      onclick: () => void handleToggleEnabled(),
      testid: `automation-enabled-toggle-${trigger.id}`,
    },
    {
      id: "history",
      label: $t("automations.view_history"),
      icon: History,
      onclick: () => onlogs(trigger.id),
      testid: `automation-history-${trigger.id}`,
    },
    {
      id: "delete",
      label: $t("automations.delete"),
      icon: Trash2,
      variant: "destructive",
      onclick: () => ondelete(trigger.id),
      testid: `automation-delete-${trigger.id}`,
    },
  ]);
</script>

<div
  class="group px-4 py-3 flex items-center gap-2.5 border-b border-border/60 text-[12px] hover:bg-muted/40 transition-colors"
  data-testid="automation-row-{trigger.id}"
  data-status={status}
>
  <!-- AUTOMATISATION : title + schedule -->
  <div class="flex-[2] min-w-0">
    <div class="font-medium text-foreground truncate" title={humanTitle}>
      {humanTitle}
    </div>
    <div class="flex items-center gap-1.5 text-[10.5px] text-muted-foreground/80 mt-px">
      <ScheduleIcon size={11} strokeWidth={1.75} class="shrink-0" aria-hidden="true" />
      <span class="truncate" title={humanized.isCustom ? trigger.source_config : ""}>
        {humanized.label}
      </span>
    </div>
  </div>

  <!-- ASSISTANT -->
  <div class="w-[160px] flex items-center gap-1.5 min-w-0">
    <Avatar name={trigger.agent} size="sm" />
    <span class="text-[11.5px] text-muted-foreground truncate">{trigger.agent}</span>
  </div>

  <!-- PROCHAIN -->
  <div class="w-[160px] text-[11px] text-muted-foreground truncate" data-testid="automation-next-run">
    {nextRunLabel}
  </div>

  <!-- STATUT -->
  <div class="w-[110px]">
    <Badge size="sm" variant={STATUS_TONE[status]}>
      {#snippet icon()}
        <StatusDot color={STATUS_DOT[status]} glow={status === "active"} />
      {/snippet}
      {statusLabel}
    </Badge>
  </div>

  <!-- DERNIÈRE EXÉC -->
  <div class="w-[90px] text-[10.5px] text-muted-foreground text-right font-mono">
    {lastRunLabel}
  </div>

  <!-- ACTIONS -->
  <div class="w-[64px] flex items-center justify-end gap-1 shrink-0">
    <button
      type="button"
      class="p-1 rounded-md text-muted-foreground hover:bg-primary/10 hover:text-primary transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
      onclick={handleFire}
      disabled={firing || !trigger.enabled}
      title={firing ? $t("automations.running") : $t("automations.run_now")}
      aria-label={firing ? $t("automations.running") : $t("automations.run_now")}
      data-testid="automation-run-now-{trigger.id}"
    >
      <Play size={13} strokeWidth={2} class="fill-current" aria-hidden="true" />
    </button>

    <ActionMenu
      items={menuItems}
      align="end"
      class="w-44"
      triggerLabel={$t("a11y.actions_menu")}
      data-testid="automation-menu-{trigger.id}"
    >
      {#snippet triggerSlot(props)}
        <button
          type="button"
          {...props}
          class="p-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:bg-muted/70 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          aria-label={$t("a11y.actions_menu")}
          data-testid="automation-menu-{trigger.id}"
        >
          <MoreHorizontal size={14} />
        </button>
      {/snippet}
    </ActionMenu>
  </div>
</div>
