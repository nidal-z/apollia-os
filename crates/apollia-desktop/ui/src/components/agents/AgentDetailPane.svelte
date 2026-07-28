<script lang="ts">
  /**
   * AgentDetailPane - the right-hand detail pane router.
   *
   * Given the current selection, renders the matching detail: the pinned
   * Apollia Chat config, a package detail, an agent header + tabs, or the
   * loading / empty states. Keeps the route shell free of the branch logic.
   */
  import { t } from "svelte-i18n";
  import { Bot, Plus } from "lucide-svelte";
  import { EmptyState } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import ApolliaChatDetail from "./ApolliaChatDetail.svelte";
  import PackageDetail from "./PackageDetail.svelte";
  import AgentDetailHeader from "./AgentDetailHeader.svelte";
  import AgentDetailTabs from "./AgentDetailTabs.svelte";
  import type { AgentActions } from "./useAgentActions.svelte";
  import type {
    AgentListItem,
    AgentPackageDetailView,
    AgentPackageListItem,
  } from "$lib/types";

  type AgentTab = "overview" | "tools" | "memory" | "activity" | "settings";

  interface Props {
    selected: AgentListItem | null;
    selectedPackage: AgentPackageListItem | null;
    apolliaChatSelected: boolean;
    pkgDetail: AgentPackageDetailView | null;
    connecting: boolean;
    agentActions: AgentActions;
    agentDetailTab: AgentTab;
    onStartChat: (name: string) => void;
    onOpenLogs: (agentId: string) => void;
    onNavigateMemory: () => void;
    onNavigateTasks: (taskId: string) => void;
    onSelectAgent: (name: string) => void;
    onInstallAgent: () => void;
    onUninstallPackage: (name: string) => void;
  }

  let {
    selected,
    selectedPackage,
    apolliaChatSelected,
    pkgDetail,
    connecting,
    agentActions,
    agentDetailTab = $bindable(),
    onStartChat,
    onOpenLogs,
    onNavigateMemory,
    onNavigateTasks,
    onSelectAgent,
    onInstallAgent,
    onUninstallPackage,
  }: Props = $props();
</script>

{#if apolliaChatSelected}
  <ApolliaChatDetail />
{:else if selectedPackage}
  <PackageDetail
    pkg={selectedPackage}
    detail={pkgDetail}
    {agentActions}
    {onSelectAgent}
    onUninstall={onUninstallPackage}
  />
{:else if selected}
  <AgentDetailHeader agent={selected} {onStartChat} />
  <AgentDetailTabs
    agent={selected}
    bind:tab={agentDetailTab}
    {onOpenLogs}
    {onNavigateMemory}
    {onNavigateTasks}
  />
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
