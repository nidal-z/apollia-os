<script lang="ts">
  /**
   * Humanized schedule line for an AutomationCard.
   *
   * Renders the natural-language sentence returned by `humanizeSchedule`.
   * When the schedule is `isCustom`, the raw cron/interval expression
   * is surfaced through a tooltip — operator never sees the digits inline.
   */
  import { t } from "svelte-i18n";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { humanizeSchedule, type ScheduleKind } from "$lib/automations/humanize";
  import { Calendar, Clock, FolderSync, Webhook, Zap } from "lucide-svelte";

  interface Props {
    kind: ScheduleKind | string;
    config: string;
    locale: string;
  }

  let { kind, config, locale }: Props = $props();

  const humanized = $derived(humanizeSchedule(kind, config, locale));

  const Icon = $derived(
    kind === "cron" ? Calendar :
    kind === "interval" ? Clock :
    kind === "file_watch" ? FolderSync :
    kind === "webhook" ? Webhook :
    Zap,
  );
</script>

<div class="flex items-center gap-1.5 text-xs text-muted-foreground" data-testid="automation-schedule-label">
  <Icon size={14} strokeWidth={1.75} class="shrink-0" aria-hidden="true" />
  {#if humanized.isCustom}
    <Tooltip content={$t("automations.custom_tooltip", { values: { expr: config } })}>
      <span class="underline decoration-dotted underline-offset-2">{humanized.label}</span>
    </Tooltip>
  {:else}
    <span>{humanized.label}</span>
  {/if}
</div>
