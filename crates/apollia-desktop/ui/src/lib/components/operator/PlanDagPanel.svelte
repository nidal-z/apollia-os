<script lang="ts">
  // Live plan DAG panel for the chat right rail.
  //
  // Reads the session-scoped plan from the single source of truth
  // (`chatPlanState` in `$lib/stores/chatPlanMode`), which owns the one
  // `runtime-event` listener filtered by session. This panel does not open a
  // second listener: it starts the shared listener for its `sessionId` and
  // tears it down on cleanup. Nodes and edges are recomputed by `$derived`
  // whenever the plan changes, so a `PlanUpdated` event reflows the graph with
  // no manual reload.
  import {
    SvelteFlow,
    Background,
    Controls,
    type Edge,
    type NodeTypes,
  } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { t } from "svelte-i18n";
  import { themeMode } from "$lib/stores/theme";
  import {
    chatPlanState,
    startChatPlanListener,
    resetChatPlan,
  } from "$lib/stores/chatPlanMode";
  import { layoutPlan, type PlanDagNode } from "./planDagLayout";
  import PlanStepNode from "./PlanStepNode.svelte";

  interface Props {
    sessionId: string;
  }
  let { sessionId }: Props = $props();

  const nodeTypes: NodeTypes = { planStep: PlanStepNode };

  const plan = $derived($chatPlanState.plan);
  const hasPlan = $derived(plan !== null && plan.steps.length > 0);

  // SvelteFlow binds `nodes`/`edges` ($bindable) and mutates them internally
  // (measured dimensions, selection). They live as `$state` and are re-derived
  // from the plan whenever it changes, so a `PlanUpdated` reflows the graph.
  let nodes = $state<PlanDagNode[]>([]);
  let edges = $state<Edge[]>([]);

  $effect(() => {
    if (plan && plan.steps.length > 0) {
      const next = layoutPlan(plan);
      nodes = next.nodes;
      edges = next.edges;
    } else {
      nodes = [];
      edges = [];
    }
  });

  $effect(() => {
    const session = sessionId;
    let unlisten: (() => void) | null = null;
    let disposed = false;
    startChatPlanListener(session).then((fn) => {
      if (disposed) {
        fn();
        return;
      }
      unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
      resetChatPlan();
    };
  });
</script>

{#if hasPlan}
  <div class="h-full w-full" data-testid="plan-dag-canvas">
    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      colorMode={$themeMode}
      fitView
      nodesDraggable={false}
      nodesConnectable={false}
      elementsSelectable
    >
      <Background />
      <Controls showLock={false} />
    </SvelteFlow>
  </div>
{:else}
  <div
    class="flex h-full items-center justify-center rounded-[10px] border border-dashed border-border px-4 py-6 text-center text-[11px] text-muted-foreground"
    data-testid="plan-dag-empty"
  >
    {$t("plan_session.empty")}
  </div>
{/if}
