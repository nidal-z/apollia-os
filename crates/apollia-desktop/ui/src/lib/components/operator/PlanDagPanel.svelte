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
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { Maximize2, Minimize2 } from "lucide-svelte";
  import { themeMode } from "$lib/stores/theme";
  import {
    chatPlanState,
    startChatPlanListener,
  } from "$lib/stores/chatPlanMode";
  import {
    latestThinking,
    latestDecisionPoint,
    resetThinking,
  } from "$lib/stores/thinking";
  import { layoutPlan, type PlanDagNode } from "./planDagLayout";
  import type { SessionPlan, SessionPlanStep } from "$lib/stores/chatPlanMode";
  import type { PlanStep } from "$lib/ipc/plan";
  import PlanStepNode from "./PlanStepNode.svelte";
  import PlanStepDrawer from "./PlanStepDrawer.svelte";
  import PlanHistoryScrubber from "./PlanHistoryScrubber.svelte";

  interface Props {
    sessionId: string;
  }
  let { sessionId }: Props = $props();

  const nodeTypes: NodeTypes = { planStep: PlanStepNode };

  const livePlan = $derived($chatPlanState.plan);

  // When the history scrubber is active it supplies the reconstructed steps at
  // the selected revision; the DAG then renders that past state instead of the
  // live plan. `null` means the scrubber is inactive and the live plan rules.
  let historySteps = $state<SessionPlanStep[] | null>(null);

  function onHistoryReconstruct(steps: PlanStep[] | null): void {
    historySteps = steps;
  }

  const plan = $derived<SessionPlan | null>(
    historySteps !== null
      ? livePlan
        ? { ...livePlan, steps: historySteps }
        : {
            plan_id: "history",
            revision: 0,
            status: "draft",
            steps: historySteps,
          }
      : livePlan,
  );
  const hasPlan = $derived(plan !== null && plan.steps.length > 0);

  let drawerOpen = $state(false);
  let selectedStep = $state<SessionPlanStep | null>(null);
  // Expanded mode lifts the cramped rail into a full-window overlay so a larger
  // plan is actually readable. The canvas remounts on toggle (`{#key expanded}`)
  // so `fitView` recomputes for the new size.
  let expanded = $state(false);

  function openStep(step: SessionPlanStep): void {
    selectedStep = step;
    drawerOpen = true;
  }

  // SvelteFlow binds `nodes`/`edges` ($bindable) and mutates them internally
  // (measured dimensions, selection). They live as `$state` and are re-derived
  // from the plan whenever it changes, so a `PlanUpdated` reflows the graph.
  let nodes = $state<PlanDagNode[]>([]);
  let edges = $state<Edge[]>([]);

  // Steps dropped by a replan: present in the prior revision, gone from the
  // current plan. Rendered as tombstones. We keep the previous step set keyed
  // by id across updates to detect removals without a second store.
  let previousSteps = $state<SessionPlanStep[]>([]);

  // Live reasoning attaches at the panel level, not per step: thinking and
  // decision events are turn-keyed with no session or step correlation in the
  // runtime, so the only honest anchor is the step currently in progress. The
  // overlay therefore rides the single in-progress node; a past revision viewed
  // through the scrubber gets nothing, since live reasoning never describes a
  // frozen snapshot.
  const liveThinking = $derived(historySteps === null ? $latestThinking : null);
  const liveDecision = $derived(
    historySteps === null ? $latestDecisionPoint : null,
  );

  $effect(() => {
    if (plan && plan.steps.length > 0) {
      // In history mode the reconstructed snapshot already holds the full set
      // at that revision, so replay-removal tombstones (a live-delta concept)
      // are suppressed to avoid showing stale ghosts on a past state.
      const liveIds = new Set(plan.steps.map((s) => s.step_id));
      // `previousSteps` is a previous-value tracker the effect also writes; read
      // it untracked so the effect never depends on what it sets (which would
      // re-trigger itself indefinitely).
      const removed =
        historySteps !== null
          ? []
          : untrack(() => previousSteps).filter((s) => !liveIds.has(s.step_id));
      const next = layoutPlan(plan, removed);
      const currentId = plan.steps.find(
        (s) => s.status === "in_progress",
      )?.step_id;
      for (const node of next.nodes) {
        node.data.onSelect = openStep;
        if (!node.data.removed && node.data.step.step_id === currentId) {
          node.data.thinking = liveThinking;
          node.data.decisionPoint = liveDecision;
        }
      }
      nodes = next.nodes;
      edges = next.edges;
      if (historySteps === null) previousSteps = plan.steps;
    } else {
      nodes = [];
      edges = [];
      if (historySteps === null) previousSteps = [];
    }
  });

  function onNodeClick({ node }: { node: PlanDagNode }): void {
    openStep(node.data.step);
  }

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
      // Release this consumer's ref on the shared listener. Do NOT reset the
      // plan store here: the approval host shares it and must keep showing the
      // card after the user leaves the plan tab.
      unlisten?.();
      resetThinking();
    };
  });
</script>

<div
  class={expanded
    ? "fixed inset-0 z-50 flex flex-col gap-2 bg-background/95 p-4 backdrop-blur"
    : "flex h-full w-full flex-col"}
>
  {#if hasPlan}
    <div class="flex items-center justify-end">
      <button
        type="button"
        onclick={() => (expanded = !expanded)}
        class="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
        aria-label={$t(expanded ? "plan_session.collapse" : "plan_session.expand")}
        data-testid="plan-dag-expand"
      >
        {#if expanded}
          <Minimize2 size={14} />
        {:else}
          <Maximize2 size={14} />
        {/if}
      </button>
    </div>
    <div class="min-h-0 flex-1" data-testid="plan-dag-canvas">
      {#key expanded}
        <SvelteFlow
          bind:nodes
          bind:edges
          {nodeTypes}
          colorMode={$themeMode}
          fitView
          fitViewOptions={{ padding: 0.2, minZoom: 0.3, maxZoom: 1.1 }}
          minZoom={0.3}
          maxZoom={1.5}
          nodesDraggable={false}
          nodesConnectable={false}
          elementsSelectable
          onnodeclick={onNodeClick}
        >
          <Background />
          <Controls showLock={false} />
        </SvelteFlow>
      {/key}
    </div>
  {:else}
    <div
      class="flex min-h-0 flex-1 items-center justify-center rounded-[10px] border border-dashed border-border px-4 py-6 text-center text-[11px] text-muted-foreground"
      data-testid="plan-dag-empty"
    >
      {$t("plan_session.empty")}
    </div>
  {/if}

  <PlanHistoryScrubber {sessionId} onreconstruct={onHistoryReconstruct} />
</div>

<PlanStepDrawer
  open={drawerOpen}
  step={selectedStep}
  onclose={() => (drawerOpen = false)}
/>
