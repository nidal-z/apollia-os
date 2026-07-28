<script lang="ts">
  /**
   * AgentListPanel - the Agents sidebar.
   *
   * Composes the canonical `SidebarHeader` (title + install actions + search)
   * with two accessible listbox sections: assistants (pinned Apollia Chat first)
   * and installed packages. Selection, filtering, and runtime actions are owned
   * by the route; this panel is presentation + event wiring only.
   */
  import { t } from "svelte-i18n";
  import { Bot, Download, Package, Plus, Search } from "lucide-svelte";
  import { SidebarHeader, SkeletonList, EmptyState } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import { Input } from "$lib/components/ui/input";
  import type { AgentListItem, AgentPackageListItem } from "$lib/types";
  import type { AgentActions } from "./useAgentActions.svelte";
  import AgentListSection from "./AgentListSection.svelte";
  import AgentPinnedRow from "./AgentPinnedRow.svelte";
  import AgentListRow from "./AgentListRow.svelte";
  import PackageListRow from "./PackageListRow.svelte";

  interface Props {
    assistants: AgentListItem[];
    packages: AgentPackageListItem[];
    query: string;
    selectedName: string | null;
    selectedPackageName: string | null;
    apolliaChatSelected: boolean;
    connecting: boolean;
    installingAgent: boolean;
    agentActions: AgentActions;
    onSelectAgent: (name: string) => void;
    onSelectPackage: (pkg: AgentPackageListItem) => void;
    onSelectApolliaChat: () => void;
    onInstallAgent: () => void;
    onInstallPackage: () => void;
    onOpenLogs: (agentId: string) => void;
    onOpenSettings: (agent: AgentListItem) => void;
    onUninstallPackage: (name: string) => void;
  }

  let {
    assistants,
    packages,
    query = $bindable(),
    selectedName,
    selectedPackageName,
    apolliaChatSelected,
    connecting,
    installingAgent,
    agentActions,
    onSelectAgent,
    onSelectPackage,
    onSelectApolliaChat,
    onInstallAgent,
    onInstallPackage,
    onOpenLogs,
    onOpenSettings,
    onUninstallPackage,
  }: Props = $props();

  const hasQuery = $derived(query.trim().length > 0);
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="agents-list">
  <SidebarHeader title={$t("agents.page_title")}>
    {#snippet actions()}
      <Button
        variant="outline"
        size="icon-sm"
        onclick={onInstallPackage}
        title={$t("agents.install_package_button")}
        aria-label={$t("agents.install_package_button")}
      >
        <Package size={13} />
      </Button>
      <Button
        variant="primary-solid"
        size="icon-sm"
        onclick={onInstallAgent}
        disabled={installingAgent}
        title={$t("agents.new_assistant")}
        aria-label={$t("agents.new_assistant")}
      >
        {#if installingAgent}<Spinner size={13} />{:else}<Download size={13} />{/if}
      </Button>
    {/snippet}
    {#snippet search()}
      <div
        class="flex items-center gap-2 rounded-md border border-border bg-surface-1 px-2.5 py-1.5"
      >
        <Search size={11} class="text-muted-foreground" />
        <Input
          type="text"
          unstyled
          bind:value={query}
          placeholder={$t("agents.filter_placeholder")}
          class="flex-1 text-caption text-foreground"
          data-testid="agents-search"
        />
      </div>
    {/snippet}
  </SidebarHeader>

  <div class="flex-1 overflow-y-auto px-2.5 pb-2">
    <AgentListSection
      title={$t("agents.section_my_assistants")}
      count={assistants.length}
      class="mt-1"
    >
      <AgentPinnedRow
        selected={apolliaChatSelected}
        onSelect={onSelectApolliaChat}
      />

      {#if connecting && assistants.length === 0}
        <SkeletonList count={4} rowClass="rounded-lg px-2.5 py-2" />
      {:else if assistants.length === 0}
        <div class="px-2 pt-6">
          <EmptyState
            tone="primary"
            title={hasQuery
              ? $t("agents.list_empty_no_result")
              : $t("agents.list_empty_no_assistant")}
            desc={hasQuery
              ? $t("agents.list_empty_try_other")
              : $t("agents.list_empty_install_first")}
          >
            {#snippet icon()}
              <Bot size={22} />
            {/snippet}
            {#snippet action()}
              {#if !hasQuery}
                <Button variant="primary-solid" size="sm" onclick={onInstallAgent}>
                  {#snippet icon()}<Plus size={12} />{/snippet}
                  {$t("agents.new_assistant")}
                </Button>
              {/if}
            {/snippet}
          </EmptyState>
        </div>
      {:else}
        {#each assistants as agent (agent.name)}
          <AgentListRow
            {agent}
            actions={agentActions}
            selected={agent.name === selectedName &&
              !apolliaChatSelected &&
              selectedPackageName === null}
            onSelect={() => onSelectAgent(agent.name)}
            {onOpenLogs}
            {onOpenSettings}
          />
        {/each}
      {/if}
    </AgentListSection>

    <AgentListSection
      title={$t("agents.section_my_packages")}
      count={packages.length}
      class="mt-4"
    >
      {#if packages.length === 0}
        <div class="px-2 py-3 text-caption text-muted-foreground/70">
          {hasQuery
            ? $t("agents.packages_empty_no_result")
            : $t("agents.packages_empty_none_installed")}
        </div>
      {:else}
        {#each packages as pkg (pkg.name)}
          <PackageListRow
            {pkg}
            actions={agentActions}
            selected={pkg.name === selectedPackageName}
            onSelect={() => onSelectPackage(pkg)}
            onUninstall={onUninstallPackage}
          />
        {/each}
      {/if}
    </AgentListSection>
  </div>
</div>
