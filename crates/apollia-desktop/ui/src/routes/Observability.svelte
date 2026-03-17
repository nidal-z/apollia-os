<script lang="ts">
  import { t } from "svelte-i18n";
  import TimelineGlobal from "../components/observability/TimelineGlobal.svelte";
  import LlmCostChart from "../components/observability/LlmCostChart.svelte";
  import AuditTrailTable from "../components/observability/AuditTrailTable.svelte";

  type ObsTab = "timeline" | "llm-costs" | "audit-trail";

  let activeTab = $state<ObsTab>("timeline");
  let timelineLoaded = $state(false);
  let costsLoaded = $state(false);
  let auditLoaded = $state(false);

  function handleTabChange(tab: ObsTab) {
    activeTab = tab;
    if (tab === "timeline") timelineLoaded = true;
    if (tab === "llm-costs") costsLoaded = true;
    if (tab === "audit-trail") auditLoaded = true;
  }

  $effect(() => {
    if (activeTab === "timeline") timelineLoaded = true;
  });
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="space-y-1">
    <h1 class="text-2xl font-bold">{$t('observability.title')}</h1>
    <p class="text-sm text-muted-foreground" data-testid="observability-subtitle">{$t('observability.subtitle')}</p>
  </div>

  <!-- Tabs -->
  <div class="flex gap-1 rounded-md glass-border glass-surface p-1">
    <button
      class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === 'timeline'
        ? 'glass-inset text-foreground shadow-sm'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => handleTabChange("timeline")}
    >
      {$t('observability.tab_timeline')}
    </button>
    <button
      class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === 'llm-costs'
        ? 'glass-inset text-foreground shadow-sm'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => handleTabChange("llm-costs")}
    >
      {$t('observability.tab_llm_costs')}
    </button>
    <button
      class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === 'audit-trail'
        ? 'glass-inset text-foreground shadow-sm'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => handleTabChange("audit-trail")}
    >
      {$t('observability.tab_audit_trail')}
    </button>
  </div>

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
