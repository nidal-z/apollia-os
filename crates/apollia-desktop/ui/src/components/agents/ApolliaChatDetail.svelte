<script lang="ts">
  /**
   * ApolliaChatDetail - the sheet of the built-in Apollia agent.
   *
   * Apollia has the same sheet as any other agent, read from its definition
   * and adjusted in place like any other. What sets it apart sits in the
   * header: it ships with the application, so it is neither exported nor
   * uninstalled.
   */
  import { t } from "svelte-i18n";
  import { Zap } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import AgentSheet from "./sheet/AgentSheet.svelte";
  import AdjustActions from "./sheet/AdjustActions.svelte";
  import { APOLLIA_AGENT_ID } from "./sheet/agentSheet";
  import { createAgentAdjust } from "./sheet/useAgentAdjust.svelte";

  interface Props {
    onOpenLogs: (agentId: string) => void;
  }

  let { onOpenLogs }: Props = $props();

  const adjust = createAgentAdjust(() => APOLLIA_AGENT_ID);
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
    <AdjustActions {adjust} />
  {/snippet}
</DetailHeader>

<div class="space-y-6 px-8 pb-8 pt-5">
  <AgentSheet definitionId={APOLLIA_AGENT_ID} agent={null} memoryNamespace={null} {onOpenLogs} {adjust} />
</div>
