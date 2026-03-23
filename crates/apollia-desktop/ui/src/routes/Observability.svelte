<script lang="ts">
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";
  import TimelineGlobal from "../components/observability/TimelineGlobal.svelte";
  import LlmCostChart from "../components/observability/LlmCostChart.svelte";
  import AuditTrailTable from "../components/observability/AuditTrailTable.svelte";

  type ObsTab = "timeline" | "llm-costs" | "audit-trail";

  let activeTab = $state<ObsTab>("timeline");
  let timelineLoaded = $state(false);
  let costsLoaded = $state(false);
  let auditLoaded = $state(false);

  function handleTabChange(key: string) {
    const tab = key as ObsTab;
    activeTab = tab;
    if (tab === "timeline") timelineLoaded = true;
    if (tab === "llm-costs") costsLoaded = true;
    if (tab === "audit-trail") auditLoaded = true;
  }

  $effect(() => {
    if (activeTab === "timeline") timelineLoaded = true;
  });
</script>

<div class="max-w-6xl space-y-6" data-testid="observability-page">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold">{$t('observability.title')}</h1>
      <p class="text-xs text-muted-foreground" data-testid="observability-subtitle">{$t('observability.subtitle')}</p>
    </div>
  </div>

  <!-- Tabs (AC-1) -->
  <TabBar
    items={[
      { key: "timeline", label: $t("observability.tab_timeline") },
      { key: "llm-costs", label: $t("observability.tab_llm_costs") },
      { key: "audit-trail", label: $t("observability.tab_audit_trail") },
    ]}
    activeTab={activeTab}
    ontabchange={handleTabChange}
    testidPrefix="observability"
  />

  <!-- Tab content (lazy-loaded on first display) -->
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
  {/if}
</div>
