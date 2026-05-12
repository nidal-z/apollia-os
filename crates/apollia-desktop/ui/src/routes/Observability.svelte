<script lang="ts">
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";
  import { uiMode } from "$lib/stores/mode";
  import { PageHeader } from "$lib/components/operator";
  import TimelineGlobal from "../components/observability/TimelineGlobal.svelte";
  import LlmCostChart from "../components/observability/LlmCostChart.svelte";
  import AuditTrailTable from "../components/observability/AuditTrailTable.svelte";
  import PlanCacheStats from "../components/observability/PlanCacheStats.svelte";
  import DelegationTree from "../components/observability/DelegationTree.svelte";

  type ObsTab = "timeline" | "llm-costs" | "audit-trail" | "delegation" | "plan-cache";

  let activeTab = $state<ObsTab>("timeline");
  let timelineLoaded = $state(false);
  let costsLoaded = $state(false);
  let auditLoaded = $state(false);
  let delegationLoaded = $state(false);
  let planCacheLoaded = $state(false);

  // Operator: 3 tabs — Timeline · Coûts · Audit (lecture non-technique).
  // Builder: 5 tabs — ajoute Delegation et Plan-Cache (inspection exhaustive).
  let tabItems = $derived.by(() => {
    const base = [
      { key: "timeline", label: $t("observability.tab_timeline") },
      { key: "llm-costs", label: $t("observability.tab_llm_costs") },
      { key: "audit-trail", label: $t("observability.tab_audit_trail") },
    ];
    if ($uiMode === "builder") {
      base.push(
        { key: "delegation", label: $t("observability.tab_delegation") },
        { key: "plan-cache", label: $t("observability.tab_plan_cache") },
      );
    }
    return base;
  });

  function handleTabChange(key: string) {
    const tab = key as ObsTab;
    activeTab = tab;
    if (tab === "timeline") timelineLoaded = true;
    if (tab === "llm-costs") costsLoaded = true;
    if (tab === "audit-trail") auditLoaded = true;
    if (tab === "delegation") delegationLoaded = true;
    if (tab === "plan-cache") planCacheLoaded = true;
  }

  $effect(() => {
    if (activeTab === "timeline") timelineLoaded = true;
  });

  $effect(() => {
    if ($uiMode !== "builder" && (activeTab === "plan-cache" || activeTab === "delegation")) {
      activeTab = "timeline";
      timelineLoaded = true;
    }
  });
</script>

<div class="mx-auto w-full max-w-6xl" data-testid="observability-page">
  <PageHeader
    kicker={$t('observability.kicker')}
    title={$t('observability.title')}
    subtitle={$t('observability.subtitle')}
  />

  <div class="px-8 pt-5">
    <TabBar
      items={tabItems}
      activeTab={activeTab}
      ontabchange={handleTabChange}
      testidPrefix="observability"
    />
  </div>

  <div class="px-8 pt-5 pb-8">
    {#if activeTab === "timeline"}
      {#if timelineLoaded}
        <TimelineGlobal />
      {/if}
    {:else if activeTab === "llm-costs"}
      {#if costsLoaded}
        <LlmCostChart />
      {/if}
    {:else if activeTab === "audit-trail"}
      {#if auditLoaded}
        <AuditTrailTable />
      {/if}
    {:else if activeTab === "delegation"}
      {#if delegationLoaded}
        <DelegationTree />
      {/if}
    {:else if activeTab === "plan-cache"}
      {#if planCacheLoaded}
        <PlanCacheStats />
      {/if}
    {/if}
  </div>
</div>
