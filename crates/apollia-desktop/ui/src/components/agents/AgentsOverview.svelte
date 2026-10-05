<script lang="ts">
  /**
   * AgentsOverview - the Agents screen on its list: the agents and the
   * workflows side by side, one column each, under one filter.
   *
   * The Agents column opens with the built-in Apollia agent, then one row per
   * agent; the Workflows column holds one row per installed workflow, with
   * its "Launch" action. An empty column says where its content comes from.
   * Under both, the examples the application proposes.
   * Selection and the gestures belong to the route; this component is
   * presentation and event wiring only.
   */
  import { t } from "svelte-i18n";
  import { Bot, FileArchive, Search, Workflow } from "lucide-svelte";
  import { EmptyState, PageHeader, SkeletonList } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import type { AgentListItem } from "$lib/types";
  import type { WorkflowListItem, WorkflowRun } from "$lib/ipc/workflows";
  import type { ExamplePackage } from "$lib/ipc/packageExamples";
  import AgentExamples from "./AgentExamples.svelte";
  import { examplesOpenByDefault } from "./agentExamples";
  import AgentListSection from "./AgentListSection.svelte";
  import AgentListRow from "./AgentListRow.svelte";
  import AgentPinnedRow from "./AgentPinnedRow.svelte";
  import WorkflowListRow from "./WorkflowListRow.svelte";

  interface Props {
    agents: AgentListItem[];
    workflows: WorkflowListItem[];
    /** The most recent run of each workflow, by workflow identifier. */
    lastRuns: Record<string, WorkflowRun | null>;
    /** The examples the application proposes, installed or not. */
    examples: ExamplePackage[];
    query: string;
    connecting: boolean;
    workflowsLoading: boolean;
    onSelectAgent: (name: string) => void;
    onSelectApollia: () => void;
    onOpenWorkflow: (workflow: WorkflowListItem, launch: boolean) => void;
    /** Choose a package file, then open its preview. */
    onInstallPackageFile: () => void;
    /** Open the preview of the example of that package name. */
    onInstallExample: (name: string) => void;
    onOpenLogs: (agentId: string) => void;
    onRequestUninstallAgent: (agent: AgentListItem) => void;
    onStartChat: (name: string) => void;
  }

  let {
    agents,
    workflows,
    lastRuns,
    examples,
    query = $bindable(),
    connecting,
    workflowsLoading,
    onSelectAgent,
    onSelectApollia,
    onOpenWorkflow,
    onInstallPackageFile,
    onInstallExample,
    onOpenLogs,
    onRequestUninstallAgent,
    onStartChat,
  }: Props = $props();

  const hasQuery = $derived(query.trim().length > 0);
  // The lists still loading are not an empty screen.
  const examplesOpen = $derived(
    !connecting && !workflowsLoading && examplesOpenByDefault(agents.length, workflows.length, hasQuery),
  );
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="agents-list">
  <PageHeader
    kicker={$t("agents.kicker")}
    title={$t("agents.title")}
    subtitle={$t("agents.subtitle")}
  >
    {#snippet actions()}
      <Button
        variant="outline"
        size="sm"
        onclick={onInstallPackageFile}
        data-testid="agents-install-apo-button"
      >
        {#snippet icon()}<FileArchive size={13} />{/snippet}
        {$t("agents.install_package")}
      </Button>
    {/snippet}
  </PageHeader>

  <div class="px-8 pt-4">
    <div class="max-w-md">
      <Input
        type="text"
        size="sm"
        icon={Search}
        bind:value={query}
        placeholder={$t("agents.filter_both")}
        aria-label={$t("agents.filter_both")}
        data-testid="agents-search"
      />
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto px-8 py-5">
    <div class="grid grid-cols-1 gap-6 md:grid-cols-2">
      <div data-testid="agents-column">
        <AgentListSection title={$t("agents.column_agents")} count={agents.length}>
          {#if !hasQuery}
            <AgentPinnedRow selected={false} onSelect={onSelectApollia} />
          {/if}
          {#if connecting && agents.length === 0}
            <SkeletonList count={4} rowClass="rounded-lg px-2.5 py-2" />
          {:else if agents.length === 0}
            <div class="px-2 pt-6" data-testid="agents-column-empty">
              <EmptyState
                tone="primary"
                title={hasQuery ? $t("agents.list_empty_no_result") : $t("agents.agents_empty_title")}
                desc={hasQuery ? $t("agents.list_empty_try_other") : $t("agents.agents_empty_desc")}
              >
                {#snippet icon()}<Bot size={22} />{/snippet}
              </EmptyState>
            </div>
          {:else}
            {#each agents as agent (agent.name)}
              <AgentListRow
                {agent}
                selected={false}
                onSelect={() => onSelectAgent(agent.name)}
                {onOpenLogs}
                onOpenSettings={(a) => onSelectAgent(a.name)}
                onRequestUninstall={onRequestUninstallAgent}
                {onStartChat}
              />
            {/each}
          {/if}
        </AgentListSection>
      </div>

      <div data-testid="workflows-column">
        <AgentListSection title={$t("agents.column_workflows")} count={workflows.length}>
          {#if workflowsLoading && workflows.length === 0}
            <SkeletonList count={3} rowClass="rounded-lg px-2.5 py-2" />
          {:else if workflows.length === 0}
            <div class="px-2 pt-6" data-testid="workflows-column-empty">
              <EmptyState
                tone="primary"
                title={hasQuery ? $t("agents.list_empty_no_result") : $t("agents.workflows_empty_title")}
                desc={hasQuery ? $t("agents.list_empty_try_other") : $t("agents.workflows_empty_desc")}
              >
                {#snippet icon()}<Workflow size={22} />{/snippet}
              </EmptyState>
            </div>
          {:else}
            {#each workflows as workflow (workflow.workflowId)}
              <WorkflowListRow
                {workflow}
                lastRun={lastRuns[workflow.workflowId] ?? null}
                onOpen={() => onOpenWorkflow(workflow, false)}
                onLaunch={() => onOpenWorkflow(workflow, true)}
              />
            {/each}
          {/if}
        </AgentListSection>
      </div>
    </div>

    <AgentExamples
      {examples}
      openByDefault={examplesOpen}
      onInstall={onInstallExample}
      onOpenSheet={onSelectAgent}
    />
  </div>
</div>
