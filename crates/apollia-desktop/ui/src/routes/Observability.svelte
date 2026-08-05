<script lang="ts" module>
  import { writable } from "svelte/store";

  /**
   * Cross-surface jump: another page asks for the timeline of one task.
   *
   * The Inbox activity rows raise it when the operator follows a failure back
   * to the run that produced it, so this tab must open already focused on that
   * task. The emitter dispatches it in the same tick as the route change, i.e.
   * before this component is mounted, so the listener lives at module scope and
   * latches the request into a store the instance reads whenever it appears.
   *
   * The latch is a one-shot. The store lives as long as the application, while
   * the route is destroyed and rebuilt on every navigation, so a request left
   * behind would reopen the timeline on that old task at each later visit and
   * override the tab the operator picked. The instance clears it as soon as it
   * applies it, which also makes two consecutive requests for the same task
   * distinguishable: the second one is again a transition away from `null`.
   */
  const FOCUS_TASK_EVENT = "apollia:observability:focus-task";

  /** Task the page must focus next, `null` when no request is pending. */
  const focusTaskRequest = writable<string | null>(null);

  if (typeof window !== "undefined") {
    window.addEventListener(FOCUS_TASK_EVENT, (event: Event) => {
      const detail = (event as CustomEvent<{ task_id?: unknown }>).detail;
      if (typeof detail?.task_id !== "string" || detail.task_id === "") return;
      focusTaskRequest.set(detail.task_id);
    });
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";
  import { uiMode } from "$lib/stores/mode";
  import { PageHeader } from "$lib/components/operator";
  import { BuilderOnly } from "$lib/components/shared";
  import { RouteTransition } from "$lib/components/ui/route-transition";
  import TimelinePanel from "../components/observability/TimelinePanel.svelte";
  import LlmCostChart from "../components/observability/LlmCostChart.svelte";
  import AuditTrailTable from "../components/observability/AuditTrailTable.svelte";
  import PlanCacheStats from "../components/observability/PlanCacheStats.svelte";
  import DelegationTree from "../components/observability/DelegationTree.svelte";
  import MailboxTable from "../components/observability/MailboxTable.svelte";
  import ActiveHooksPanel from "../components/observability/ActiveHooksPanel.svelte";
  import { markFollowVisited } from "$lib/tour/persistence";

  type ObsTab =
    | "timeline"
    | "llm-costs"
    | "audit-trail"
    | "mailbox"
    | "delegation"
    | "plan-cache"
    | "hooks";

  let activeTab = $state<ObsTab>("timeline");
  let timelineLoaded = $state(false);
  let costsLoaded = $state(false);
  let auditLoaded = $state(false);
  let mailboxLoaded = $state(false);
  let delegationLoaded = $state(false);
  let planCacheLoaded = $state(false);
  let hooksLoaded = $state(false);
  /** Task the timeline tab is focused on, `null` for the whole activity. */
  let timelineTaskId = $state<string | null>(null);

  // Applies a focus request latched at module scope, whether it landed before
  // this page was mounted or while it was already open. Consuming it here is
  // what keeps it from being replayed on the next visit to this page.
  $effect(() => {
    const requestedTaskId = $focusTaskRequest;
    if (requestedTaskId === null) return;
    focusTaskRequest.set(null);
    timelineTaskId = requestedTaskId;
    activeTab = "timeline";
    timelineLoaded = true;
  });

  // Ticks the "follow" getting-started milestone. Consulting the activity and
  // audit surface is an act, so no store can report it on our behalf.
  onMount(() => markFollowVisited());

  /** Tabs reserved to builder mode: they expose runtime internals. */
  const BUILDER_TABS: ReadonlySet<ObsTab> = new Set<ObsTab>([
    "delegation",
    "plan-cache",
    "hooks",
  ]);

  // Operator: 4 tabs - Timeline · Coûts · Audit · Messagerie (lecture non-technique).
  // Builder: 7 tabs - ajoute Delegation, Plan-Cache et Hooks (inspection exhaustive).
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
        { key: "hooks", label: $t("observability.tab_hooks") },
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
    if (tab === "hooks") hooksLoaded = true;
  }

  $effect(() => {
    if (activeTab === "timeline") timelineLoaded = true;
  });

  $effect(() => {
    if ($uiMode !== "builder" && BUILDER_TABS.has(activeTab)) {
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
            <TimelinePanel
              taskId={timelineTaskId}
              ontaskchange={(id) => (timelineTaskId = id)}
            />
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
        {:else if activeTab === "hooks"}
          {#if hooksLoaded}
            <BuilderOnly>
              <ActiveHooksPanel />
            </BuilderOnly>
          {/if}
        {/if}
      </RouteTransition>
    {/key}
  </div>
</div>
