<script lang="ts">
  /**
   * Operator-facing automation card.
   *
   * Header — avatar + title + target assistant, status badge, schedule label.
   * Body — optional description + "next run" line + success-rate stats.
   * Footer — primary "Run now" CTA + secondary "View history" link.
   *
   * No cron digits, no edit/delete buttons — those live in `/triggers`
   * builder mode only (C.T.11, A.2.1).
   */
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Play, History, AlertCircle } from "lucide-svelte";
  import type { TriggerStatus, TriggerFireResult } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { Avatar } from "$lib/components/ui/avatar";
  import { addToast } from "$lib/components/ui/toast/store";
  import {
    estimateNextRun,
    formatNextRun,
    computeSuccessRate,
  } from "$lib/automations/humanize";
  import AutomationStatusBadge from "./AutomationStatusBadge.svelte";
  import AutomationScheduleLabel from "./AutomationScheduleLabel.svelte";

  interface Props {
    trigger: TriggerStatus;
    locale: string;
    description?: string | null;
    /** Pre-computed error reason (e.g. last failed log entry). Null means no error. */
    lastError?: { message: string; firedAt: string } | null;
    onfire: (taskId: string) => void;
    onlogs: (triggerId: string) => void;
  }

  let { trigger, locale, description = null, lastError = null, onfire, onlogs }: Props = $props();

  let firing = $state(false);

  const successRate = $derived(computeSuccessRate(trigger.fire_count, trigger.skip_count));
  const totalRuns = $derived(trigger.fire_count + trigger.skip_count);

  const status: "active" | "paused" | "error" = $derived(
    !trigger.enabled ? "paused" :
    lastError ? "error" :
    "active",
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

  const skippedTooltip = $t("automations.skipped_tooltip");
  const successTooltip = $derived(
    $t("automations.success_tooltip", {
      values: { fires: trigger.fire_count, total: totalRuns },
    }),
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

  const errorTooltip = $derived(
    lastError
      ? $t("automations.error_tooltip", { values: { when: formatRelative(lastError.firedAt) } })
      : undefined,
  );

  async function handleFire() {
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

  const humanTitle = $derived(trigger.id.replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase()));
</script>

<article
  class="glass-card-hover relative overflow-hidden rounded-xl border glass-border"
  data-testid="automation-card-{trigger.id}"
  data-status={status}
>
  <div
    class="h-1 w-full {status === 'active' ? 'bg-gradient-to-r from-emerald-400 to-emerald-600' : status === 'error' ? 'bg-gradient-to-r from-red-500 to-red-600' : 'bg-muted-foreground/20'}"
    aria-hidden="true"
  ></div>

  <div class="flex flex-col gap-3 p-4">
    <!-- Header: title + status -->
    <header class="flex items-start justify-between gap-3">
      <div class="flex min-w-0 flex-1 items-center gap-2.5">
        <Avatar name={trigger.agent} size="sm" />
        <div class="min-w-0">
          <h3 class="truncate text-sm font-semibold text-foreground" data-testid="automation-title">
            {humanTitle}
          </h3>
          <p class="truncate text-[11px] text-muted-foreground">
            {$t("automations.target_prefix")} · <span class="font-medium text-foreground/80">{trigger.agent}</span>
          </p>
        </div>
      </div>
      <AutomationStatusBadge {status} errorTooltip={errorTooltip} />
    </header>

    <!-- Schedule -->
    <AutomationScheduleLabel
      kind={trigger.source_kind}
      config={trigger.source_config}
      {locale}
    />

    <!-- Optional description -->
    {#if description}
      <p class="text-xs leading-relaxed text-muted-foreground" data-testid="automation-description">
        {description}
      </p>
    {/if}

    <!-- Next run + stats -->
    <div class="flex flex-col gap-1.5 rounded-lg bg-muted/40 px-3 py-2 text-xs">
      <div class="flex items-center justify-between gap-2">
        <span class="text-muted-foreground" data-testid="automation-next-run">{nextRunLabel}</span>
        {#if status === "error"}
          <span class="inline-flex items-center gap-1 text-destructive">
            <AlertCircle size={12} strokeWidth={1.75} aria-hidden="true" />
            <span>{$t("automations.last_run_failed")}</span>
          </span>
        {/if}
      </div>
      <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground">
        <Tooltip content={successTooltip}>
          <span data-testid="automation-success-rate">
            {$t("automations.success_stat", { values: { rate: successRate, fires: trigger.fire_count, total: totalRuns } })}
          </span>
        </Tooltip>
        {#if trigger.skip_count > 0}
          <Tooltip content={skippedTooltip}>
            <span class="underline decoration-dotted underline-offset-2" data-testid="automation-skipped">
              {$t("automations.skipped_stat", { values: { count: trigger.skip_count } })}
            </span>
          </Tooltip>
        {/if}
        {#if trigger.last_fired}
          <span data-testid="automation-last-run">
            {$t("automations.last_run", { values: { when: formatRelative(trigger.last_fired) } })}
          </span>
        {/if}
      </div>
    </div>

    <!-- Footer actions -->
    <footer class="flex flex-col gap-2 sm:flex-row sm:items-center">
      <Button
        variant="primary-solid"
        size="sm"
        onclick={handleFire}
        disabled={firing || !trigger.enabled}
        class="w-full sm:w-auto"
        data-testid="automation-run-now-{trigger.id}"
      >
        <Play size={14} strokeWidth={2} class="mr-1.5 fill-current" aria-hidden="true" />
        {firing ? $t("automations.running") : $t("automations.run_now")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        onclick={() => onlogs(trigger.id)}
        class="w-full sm:w-auto"
        data-testid="automation-history-{trigger.id}"
      >
        <History size={14} strokeWidth={1.75} class="mr-1.5" aria-hidden="true" />
        {$t("automations.view_history")}
      </Button>
    </footer>
  </div>
</article>
