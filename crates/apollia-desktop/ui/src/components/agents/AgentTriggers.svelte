<script lang="ts">
  import { t } from "svelte-i18n";
  import type { TriggerStatus } from "$lib/types";
  import { triggers } from "$lib/stores/triggers";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import { Timer, Eye, Folder, Globe, Zap } from "lucide-svelte";

  interface Props {
    agentName: string;
  }

  let { agentName }: Props = $props();

  let agentTriggers = $derived(
    $triggers.filter((tr) => tr.agent === agentName),
  );

  const SOURCE_ICON: Record<TriggerStatus["source_kind"], typeof Timer> = {
    cron: Timer,
    interval: Timer,
    file_watch: Folder,
    webhook: Globe,
    oneshot: Zap,
  };

  const SOURCE_BADGE_CLASS: Record<TriggerStatus["source_kind"], string> = {
    cron: "border-info text-info",
    interval: "border-info text-info-foreground",
    file_watch: "border-success text-success",
    webhook: "border-accent text-accent-foreground",
    oneshot: "border-warning text-warning",
  };

  function humanizeTrigger(trigger: TriggerStatus): string {
    switch (trigger.source_kind) {
      case "cron":
        return $t('agent_detail.trigger_cron');
      case "interval":
        return $t('agent_detail.trigger_interval');
      case "file_watch":
        return $t('agent_detail.trigger_file_watch');
      case "webhook":
        return $t('agent_detail.trigger_webhook');
      case "oneshot":
        return $t('agent_detail.trigger_oneshot');
      default:
        return trigger.source_kind;
    }
  }

  function triggerConfigDetail(trigger: TriggerStatus): string {
    switch (trigger.source_kind) {
      case "cron":
        return $t('agent_detail.trigger_config_cron', { values: { config: trigger.source_config } });
      case "interval":
        return $t('agent_detail.trigger_config_interval', { values: { config: trigger.source_config } });
      case "file_watch":
        return $t('agent_detail.trigger_config_file_watch', { values: { config: trigger.source_config } });
      case "webhook":
        return $t('agent_detail.trigger_config_webhook', { values: { config: trigger.source_config } });
      case "oneshot":
        return $t('agent_detail.trigger_config_oneshot');
      default:
        return trigger.source_config;
    }
  }

  function formatRelativeDate(isoDate: string): string {
    const date = new Date(isoDate);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMin = Math.floor(diffMs / 60_000);
    if (diffMin < 1) return "< 1 min";
    if (diffMin < 60) return `${diffMin}m`;
    const diffH = Math.floor(diffMin / 60);
    if (diffH < 24) return `${diffH}h`;
    const diffD = Math.floor(diffH / 24);
    return `${diffD}j`;
  }
</script>

<section data-testid="agent-triggers-section">
  <h3 class="mb-3 text-sm font-medium">{$t('agent_detail.triggers_title')}</h3>

  {#if agentTriggers.length === 0}
    <p class="text-xs text-muted-foreground">{$t('agent_detail.no_triggers')}</p>
  {:else}
    <div class="space-y-2">
      {#each agentTriggers as trigger (trigger.id)}
        {@const IconComponent = SOURCE_ICON[trigger.source_kind] ?? Eye}
        <div
          class="rounded-md border px-3 py-2.5"
          data-testid="agent-detail-trigger-row"
          data-trigger-id={trigger.id}
        >
          <div class="flex items-center gap-3">
            <IconComponent size={14} class="shrink-0 text-muted-foreground" />

            <div class="min-w-0 flex-1">
              <span class="text-sm">{humanizeTrigger(trigger)}</span>
            </div>

            <div class="flex shrink-0 items-center gap-2">
              {#if trigger.enabled}
                <Badge variant="outline" class="text-micro px-1.5 py-0 border-success/50 text-success">
                  {$t('agent_detail.trigger_enabled')}
                </Badge>
              {:else}
                <Badge variant="secondary" class="text-micro px-1.5 py-0">
                  {$t('agent_detail.trigger_disabled')}
                </Badge>
              {/if}
            </div>
          </div>

          <!-- Detail row: config + stats -->
          <div class="mt-1.5 flex items-center gap-3 text-caption text-muted-foreground">
            <span class="truncate">{triggerConfigDetail(trigger)}</span>
            <span class="shrink-0">{trigger.fire_count} {$t('triggers.fires')}</span>
            {#if trigger.last_fired}
              <span class="shrink-0">{$t('agent_detail.trigger_last_fired', { values: { date: formatRelativeDate(trigger.last_fired) } })}</span>
            {/if}
          </div>

          {#if $uiMode === "builder"}
            <div class="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
              <Badge variant="outline" class={SOURCE_BADGE_CLASS[trigger.source_kind] ?? ""}>
                {trigger.source_kind.toUpperCase()}
              </Badge>
              <code class="text-micro">{trigger.id}</code>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</section>
