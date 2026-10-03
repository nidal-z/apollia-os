<script lang="ts">
  /**
   * AgentDetailPane - the right-hand detail pane router.
   *
   * Given the current selection, renders the matching detail: the sheet of
   * the built-in Apollia agent, a package detail, the header and sheet of an
   * agent, or the loading / empty states. Keeps the route shell free of the
   * branch logic.
   */
  import { t } from "svelte-i18n";
  import { Bot, Plus } from "lucide-svelte";
  import { EmptyState } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import ApolliaChatDetail from "./ApolliaChatDetail.svelte";
  import PackageDetail from "./PackageDetail.svelte";
  import AgentDetailHeader from "./AgentDetailHeader.svelte";
  import AgentSheet from "./sheet/AgentSheet.svelte";
  import type { AgentActions } from "./useAgentActions.svelte";
  import type {
    AgentListItem,
    AgentPackageDetailView,
    AgentPackageListItem,
  } from "$lib/types";

  interface Props {
    selected: AgentListItem | null;
    selectedPackage: AgentPackageListItem | null;
    apolliaChatSelected: boolean;
    pkgDetail: AgentPackageDetailView | null;
    connecting: boolean;
    agentActions: AgentActions;
    onStartChat: (name: string) => void;
    onOpenLogs: (agentId: string) => void;
    onSelectAgent: (name: string) => void;
    onInstallAgent: () => void;
    confirmingUninstall: boolean;
    onArmUninstall: () => void;
    onDisarmUninstall: () => void;
    onUninstallPackage: (name: string) => void;
  }

  let {
    selected,
    selectedPackage,
    apolliaChatSelected,
    pkgDetail,
    connecting,
    agentActions,
    onStartChat,
    onOpenLogs,
    onSelectAgent,
    onInstallAgent,
    confirmingUninstall,
    onArmUninstall,
    onDisarmUninstall,
    onUninstallPackage,
  }: Props = $props();
</script>

{#if apolliaChatSelected}
  <ApolliaChatDetail {onOpenLogs} />
{:else if selectedPackage}
  <PackageDetail
    pkg={selectedPackage}
    detail={pkgDetail}
    {onSelectAgent}
    onUninstall={onUninstallPackage}
  />
{:else if selected}
  <AgentDetailHeader
    agent={selected}
    {agentActions}
    {confirmingUninstall}
    {onArmUninstall}
    {onDisarmUninstall}
    {onStartChat}
  />
  <div class="px-8 pb-8 pt-5" data-testid="agent-detail-content">
    {#key selected.name}
      <AgentSheet
        definitionId={selected.name}
        agent={selected}
        memoryNamespace={selected.memory_namespace}
        {onOpenLogs}
      />
    {/key}
  </div>
{:else if connecting}
  <div class="flex flex-1 items-center justify-center px-8">
    <div class="text-body-xs text-muted-foreground">
      {$t("agents.loading_assistants")}
    </div>
  </div>
{:else}
  <div class="flex flex-1 items-center justify-center px-8 py-14">
    <EmptyState
      tone="primary"
      title={$t("agents.detail_empty_title")}
      desc={$t("agents.list_empty_install_first")}
    >
      {#snippet icon()}<Bot size={22} />{/snippet}
      {#snippet action()}
        <Button variant="primary-solid" size="sm" onclick={onInstallAgent}>
          {#snippet icon()}<Plus size={12} />{/snippet}
          {$t("agents.new_assistant")}
        </Button>
      {/snippet}
    </EmptyState>
  </div>
{/if}
