<script lang="ts">
  /**
   * AgentDetailPane - the sheet of the agent the screen is open on: the
   * built-in Apollia agent, or an agent with its header and its sheet, under
   * a breadcrumb that leads back to the list.
   */
  import { t } from "svelte-i18n";
  import { ChevronRight } from "lucide-svelte";
  import ApolliaChatDetail from "./ApolliaChatDetail.svelte";
  import AgentDetailHeader from "./AgentDetailHeader.svelte";
  import AgentSheet from "./sheet/AgentSheet.svelte";
  import { createAgentAdjust } from "./sheet/useAgentAdjust.svelte";
  import type { AgentActions } from "./useAgentActions.svelte";
  import type { AgentListItem } from "$lib/types";

  interface Props {
    /** The agent shown, `null` for the built-in Apollia agent. */
    selected: AgentListItem | null;
    agentActions: AgentActions;
    onStartChat: (name: string) => void;
    onOpenLogs: (agentId: string) => void;
    confirmingUninstall: boolean;
    onArmUninstall: () => void;
    onDisarmUninstall: () => void;
    onback: () => void;
  }

  let {
    selected,
    agentActions,
    onStartChat,
    onOpenLogs,
    confirmingUninstall,
    onArmUninstall,
    onDisarmUninstall,
    onback,
  }: Props = $props();

  // The adjustment of the selected agent; a change of selection ends it.
  const adjust = createAgentAdjust(() => selected?.name ?? "");
</script>

<nav
  class="flex items-center gap-1 px-8 pt-5 text-caption text-muted-foreground"
  aria-label={$t("a11y.breadcrumb")}
>
  <button
    type="button"
    class="rounded px-1 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    onclick={onback}
    data-testid="agent-sheet-back"
  >
    {$t("agents.title")}
  </button>
  <ChevronRight size={12} />
  <span class="text-foreground">{$t("agents.kicker_agent")}</span>
</nav>

{#if selected === null}
  <ApolliaChatDetail {onOpenLogs} />
{:else}
  <AgentDetailHeader
    agent={selected}
    {agentActions}
    {confirmingUninstall}
    {onArmUninstall}
    {onDisarmUninstall}
    {onStartChat}
    {adjust}
  />
  <div class="px-8 pb-8 pt-5" data-testid="agent-detail-content">
    {#key selected.name}
      <AgentSheet
        definitionId={selected.name}
        agent={selected}
        memoryNamespace={selected.memory_namespace}
        {onOpenLogs}
        {adjust}
      />
    {/key}
  </div>
{/if}
