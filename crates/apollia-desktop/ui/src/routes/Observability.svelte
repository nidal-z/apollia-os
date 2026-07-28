<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";
  import { uiMode } from "$lib/stores/mode";
  import { PageHeader } from "$lib/components/operator";
  import { RouteTransition } from "$lib/components/ui/route-transition";
  import TimelineGlobal from "../components/observability/TimelineGlobal.svelte";
  import LlmCostChart from "../components/observability/LlmCostChart.svelte";
  import AuditTrailTable from "../components/observability/AuditTrailTable.svelte";
  import PlanCacheStats from "../components/observability/PlanCacheStats.svelte";
  import DelegationTree from "../components/observability/DelegationTree.svelte";
  import MailboxTable from "../components/observability/MailboxTable.svelte";
  import { markFollowVisited } from "$lib/tour/persistence";

  // Ticks the "follow" getting-started milestone. Consulting the activity and
  // audit surface is an act, so no store can report it on our behalf.
  onMount(() => markFollowVisited());

  type ObsTab =
    | "timeline"
    | "llm-costs"
    | "audit-trail"
    | "mailbox"
    | "delegation"
    | "plan-cache";

  let activeTab = $state<ObsTab>("timeline");
  let timelineLoaded = $state(false);
  let costsLoaded = $state(false);
  let auditLoaded = $state(false);
  let mailboxLoaded = $state(false);
  let delegationLoaded = $state(false);
  let planCacheLoaded = $state(false);

  // Operator: 4 tabs - Timeline · Coûts · Audit · Messagerie (lecture non-technique).
  // Builder: 6 tabs - ajoute Delegation et Plan-Cache (inspection exhaustive).
  let tabItems = $derived.by(() => {
    const base = [
      { key: "timeline", label: $t("observability.tab_timeline") },
      { key: "llm-costs", label: $t("observability.tab_llm_costs") },
      { key: "audit-trail", label: $t("observability.tab_audit_trail") },
      { key: "mailbox", label: $t("observability.tab_mailbox") },
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
    if (tab === "mailbox") mailboxLoaded = true;
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
    title={$t('observability.title')}
    subtitle={$t('observability.subtitle')}
  />

  <div class="px-8 pt-5">
    <TabBar
      variant="underline"
      items={tabItems}
      activeTab={activeTab}
      ontabchange={handleTabChange}
      testidPrefix="observability"
    />
  </div>

  <!-- Keyed on activeTab so each tab body re-runs the restrained fly+fade swap
       (RouteTransition), replacing the previous hard snap between panels. -->
  <div class="px-8 pt-5 pb-8">
    {#key activeTab}
      <RouteTransition>
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
        {:else if activeTab === "mailbox"}
          {#if mailboxLoaded}
            <MailboxTable />
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
      </RouteTransition>
    {/key}
  </div>
</div>
