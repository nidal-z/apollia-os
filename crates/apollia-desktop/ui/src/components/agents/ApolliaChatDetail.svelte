<script lang="ts">
  /**
   * ApolliaChatDetail - the sheet of the built-in Apollia agent.
   *
   * Apollia has the same sheet as any other agent, read from its definition.
   * What sets it apart sits in the header: it ships with the application, so
   * it is neither exported nor uninstalled. "Adjust" opens its settings panel
   * below the header until the sheet becomes editable for every agent.
   */
  import { t } from "svelte-i18n";
  import { SlidersHorizontal, Zap } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import ApolliaChatConfigPanel from "./ApolliaChatConfigPanel.svelte";
  import AgentSheet from "./sheet/AgentSheet.svelte";
  import { APOLLIA_AGENT_ID } from "./sheet/agentSheet";

  interface Props {
    onOpenLogs: (agentId: string) => void;
  }

  let { onOpenLogs }: Props = $props();

  let adjusting = $state(false);
</script>

<DetailHeader title="Apollia" titleTestid="agent-detail-title" meta={$t("agent_sheet.apollia_meta")}>
  {#snippet leading()}
    <span
      class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gradient-primary text-primary-foreground shadow-elev-2"
    >
      <Zap size={18} />
    </span>
  {/snippet}
  {#snippet actions()}
    <Button
      variant={adjusting ? "primary-solid" : "outline"}
      size="sm"
      aria-pressed={adjusting}
      onclick={() => (adjusting = !adjusting)}
      data-testid="apollia-sheet-adjust"
    >
      {#snippet icon()}<SlidersHorizontal size={12} />{/snippet}
      {adjusting ? $t("agent_sheet.adjust_close") : $t("agent_sheet.adjust")}
    </Button>
  {/snippet}
</DetailHeader>

<div class="space-y-6 px-8 pb-8 pt-5">
  {#if adjusting}
    <ApolliaChatConfigPanel />
  {/if}
  <AgentSheet definitionId={APOLLIA_AGENT_ID} agent={null} memoryNamespace={null} {onOpenLogs} />
</div>
