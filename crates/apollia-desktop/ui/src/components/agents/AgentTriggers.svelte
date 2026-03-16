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

  /** Translate a trigger's source_kind into human-readable text. */
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
</script>

<section data-testid="agent-detail-triggers">
  <h3 class="mb-3 text-sm font-semibold">{$t('agent_detail.triggers_title')}</h3>

  {#if agentTriggers.length === 0}
    <p class="text-xs text-muted-foreground">{$t('agent_detail.no_triggers')}</p>
  {:else}
    <div class="space-y-2">
      {#each agentTriggers as trigger (trigger.id)}
        {@const IconComponent = SOURCE_ICON[trigger.source_kind] ?? Eye}
        <div
          class="flex items-center gap-3 rounded-md border px-3 py-2"
          data-testid="agent-detail-trigger-row"
          data-trigger-id={trigger.id}
        >
          <IconComponent size={14} class="shrink-0 text-muted-foreground" />

          <div class="min-w-0 flex-1">
            <span class="text-sm">{humanizeTrigger(trigger)}</span>
            {#if $uiMode === "builder"}
              <div class="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground">
                <Badge variant="outline" class={SOURCE_BADGE_CLASS[trigger.source_kind] ?? ""}>
                  {trigger.source_kind.toUpperCase()}
                </Badge>
                <code class="text-[10px]">{trigger.id}</code>
              </div>
            {/if}
          </div>

          <div class="flex items-center gap-3 text-xs text-muted-foreground">
            <span>{trigger.fire_count} {$t('triggers.fires')}</span>
            {#if !trigger.enabled}
              <Badge variant="secondary" class="text-[10px]">{$t('agent_detail.trigger_disabled')}</Badge>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>
