<script lang="ts">
  /**
   * PackageDetail - detail pane for a selected package.
   *
   * Header (signature-gradient icon, uninstall with a two-step confirm,
   * aggregated status badges) plus an underline TabBar routing to the
   * overview / agents / triggers panes. A package is never started nor
   * stopped: its agents are loaded by their first task.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle, Package, Trash2 } from "lucide-svelte";
  import { DetailHeader, StatusDot } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { TabBar } from "$lib/components/ui/tabs";
  import { RouteTransition } from "$lib/components/ui/route-transition";
  import { agents } from "$lib/stores/agents";
  import { triggers } from "$lib/stores/sse";
  import { packageRuntimeState } from "$lib/stores/agentPackages";
  import {
    packageStatusColor,
    packageStatusLabel,
    packageStatusTone,
  } from "./agentStatus";
  import PackageOverviewTab from "./tabs/PackageOverviewTab.svelte";
  import PackageAgentsTab from "./tabs/PackageAgentsTab.svelte";
  import PackageTriggersTab from "./tabs/PackageTriggersTab.svelte";
  import type { AgentPackageDetailView, AgentPackageListItem } from "$lib/types";

  type PackageTab = "overview" | "agents" | "triggers";

  interface Props {
    pkg: AgentPackageListItem;
    detail: AgentPackageDetailView | null;
    onSelectAgent: (name: string) => void;
    onUninstall: (name: string) => void;
  }

  let { pkg, detail, onSelectAgent, onUninstall }: Props = $props();

  let tab = $state<PackageTab>("overview");
  let confirmUninstall = $state(false);

  // svelte-ignore state_referenced_locally
  let lastName = $state(pkg.name);
  $effect(() => {
    if (pkg.name !== lastName) {
      lastName = pkg.name;
      tab = "overview";
      confirmUninstall = false;
    }
  });

  const pkgState = $derived(packageRuntimeState(pkg, $agents, $triggers));
  const triggerCount = $derived(
    (() => {
      const names = new Set(pkg.agents.map((a) => a.name));
      return $triggers.filter((tr) => names.has(tr.agent)).length;
    })(),
  );
</script>

<DetailHeader
  title={pkg.name}
  titleTestid="package-detail-title"
  meta={pkg.description || $t("agents.package_default_description")}
>
  {#snippet leading()}
    <span
      class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg {pkgState.status ===
      'available'
        ? 'bg-gradient-primary text-primary-foreground shadow-elev-2'
        : 'border border-border bg-surface-1 text-primary'}"
    >
      <Package size={18} />
    </span>
  {/snippet}
  {#snippet actions()}
    {#if confirmUninstall}
      <Button variant="outline" size="sm" onclick={() => (confirmUninstall = false)}>
        {$t("common.cancel")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={() => onUninstall(pkg.name)}>
        {#snippet icon()}<Trash2 size={12} />{/snippet}
        {$t("common.confirm")}
      </Button>
    {:else}
      <Button variant="outline" size="sm" onclick={() => (confirmUninstall = true)}>
        {#snippet icon()}<Trash2 size={12} />{/snippet}
        {$t("agents.uninstall")}
      </Button>
    {/if}
  {/snippet}
  {#snippet footer()}
    <Badge size="sm" variant={packageStatusTone(pkgState)}>
      {#snippet icon()}
        <StatusDot
          color={packageStatusColor(pkgState)}
          glow={pkgState.status === "available"}
          size={5}
        />
      {/snippet}
      {packageStatusLabel(pkgState, $t)}
    </Badge>
    <Badge size="sm" variant="neutral">v{pkg.version}</Badge>
    <Badge size="sm" variant="neutral">
      {pkgState.availableAgents}/{pkgState.totalAgents} {$t("agents.agents_word")}
    </Badge>
    <Badge size="sm" variant="neutral">
      {pkgState.enabledTriggers}/{pkgState.totalTriggers} {$t("agents.triggers_word")}
    </Badge>
    {#if pkg.root_missing}
      <Badge size="sm" variant="warning">
        {#snippet icon()}<AlertTriangle size={10} />{/snippet}
        {$t("agents.source_missing")}
      </Badge>
    {/if}
  {/snippet}
</DetailHeader>

<div class="px-8 pt-3.5">
  <TabBar
    variant="underline"
    testidPrefix="package-detail"
    activeTab={tab}
    items={[
      { key: "overview", label: $t("agents.tab_overview") },
      { key: "agents", label: $t("agents.tab_agents"), count: pkg.agents.length },
      { key: "triggers", label: $t("agents.tab_triggers"), count: triggerCount },
    ]}
    ontabchange={(key) => (tab = key as PackageTab)}
  />
</div>

<div class="px-8 pb-8 pt-5" data-testid="package-detail-tab-content">
  {#key tab}
    <RouteTransition>
      {#if tab === "overview"}
        <PackageOverviewTab {pkg} {detail} />
      {:else if tab === "agents"}
        <PackageAgentsTab {pkg} {onSelectAgent} />
      {:else if tab === "triggers"}
        <PackageTriggersTab {pkg} />
      {/if}
    </RouteTransition>
  {/key}
</div>
