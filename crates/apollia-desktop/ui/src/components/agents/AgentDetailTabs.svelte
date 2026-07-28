<script lang="ts">
  /**
   * AgentDetailTabs - the tab bar and routed content for a selected assistant.
   *
   * The active tab is bound by the route (so it can reset on selection change
   * and jump to a specific tab for the guided tour). Panes swap through the
   * canonical `RouteTransition` (fly + fade, reduced motion aware); each pane is
   * its own component under `tabs/`.
   */
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";
  import { RouteTransition } from "$lib/components/ui/route-transition";
  import type { AgentListItem } from "$lib/types";
  import AgentOverviewTab from "./tabs/AgentOverviewTab.svelte";
  import AgentToolsTab from "./tabs/AgentToolsTab.svelte";
  import AgentMemoryTab from "./tabs/AgentMemoryTab.svelte";
  import AgentActivityTab from "./tabs/AgentActivityTab.svelte";
  import AgentSettingsTab from "./tabs/AgentSettingsTab.svelte";

  type AgentTab = "overview" | "tools" | "memory" | "activity" | "settings";

  interface Props {
    agent: AgentListItem;
    tab: AgentTab;
    onOpenLogs: (agentId: string) => void;
    onNavigateMemory: () => void;
    onNavigateTasks: (taskId: string) => void;
  }

  let {
    agent,
    tab = $bindable(),
    onOpenLogs,
    onNavigateMemory,
    onNavigateTasks,
  }: Props = $props();

  const toolCount = $derived(
    agent.tools_required.length + agent.tools_optional.length,
  );
</script>

<div class="px-8 pt-3.5">
  <TabBar
    variant="underline"
    testidPrefix="agent-detail"
    activeTab={tab}
    items={[
      { key: "overview", label: $t("agents.tab_overview") },
      { key: "tools", label: $t("agents.tab_tools"), count: toolCount },
      { key: "memory", label: $t("agents.tab_memory") },
      { key: "activity", label: $t("agents.tab_activity") },
      { key: "settings", label: $t("agents.tab_settings") },
    ]}
    ontabchange={(key) => (tab = key as AgentTab)}
  />
</div>

<div class="px-8 pb-8 pt-5" data-testid="agent-detail-tab-content">
  {#key tab}
    <RouteTransition>
      {#if tab === "overview"}
        <AgentOverviewTab {agent} />
      {:else if tab === "tools"}
        <AgentToolsTab {agent} />
      {:else if tab === "memory"}
        <AgentMemoryTab {agent} {onNavigateMemory} />
      {:else if tab === "activity"}
        <AgentActivityTab {agent} onTaskClick={onNavigateTasks} />
      {:else if tab === "settings"}
        <AgentSettingsTab {agent} {onOpenLogs} />
      {/if}
    </RouteTransition>
  {/key}
</div>
