// Pure layered auto-layout for the plan DAG.
//
// Turns the session plan (steps + `depends_on` edges) into positioned xyflow
// nodes and directed edges. Layout is top-to-bottom (`rankdir: "TB"`) so a step
// always sits below the steps it depends on. No DOM, no side effect: callable
// from a `$derived` and unit-testable in isolation.

import dagre from "dagre";
import { MarkerType, type Node, type Edge } from "@xyflow/svelte";
import type { SessionPlan, SessionPlanStep } from "$lib/stores/chatPlanMode";
import type { DecisionPoint } from "$lib/types";

/** Live or frozen thinking trace attached to a step's current turn. */
export interface StepThinking {
  content: string;
  live: boolean;
}

/** Geometry of a node, in sync with the rendered card width/height. */
export const NODE_WIDTH = 220;
export const NODE_HEIGHT = 104;

/** Data carried by a plan-step node (read by `PlanStepNode`). */
export interface StepNodeData extends Record<string, unknown> {
  step: SessionPlanStep;
  /** 1-based position of the step in the plan, shown as a sequence number. */
  index: number;
  /** Set when the step was dropped by a replan (tombstone rendering). */
  removed: boolean;
  /** Opens the per-step trace drawer (keyboard activation on the card). */
  onSelect?: (step: SessionPlanStep) => void;
  /** Thinking trace for the step's current turn (Builder-only rendering). */
  thinking?: StepThinking | null;
  /** Decision point recorded on the step's turn (Builder-only rendering). */
  decisionPoint?: DecisionPoint | null;
}

export type PlanDagNode = Node<StepNodeData, "planStep">;

/**
 * Computes the layered DAG layout for a plan.
 *
 * Returns positioned nodes (type `"planStep"`) and directed dependency edges.
 * An edge is `animated` when its target step is currently in progress. Steps in
 * `removed` (dropped by a replan: present before, absent from `plan`) are added
 * as tombstone nodes after the live steps, kept clickable for their history.
 */
export function layoutPlan(
  plan: SessionPlan,
  removed: SessionPlanStep[] = [],
): {
  nodes: PlanDagNode[];
  edges: Edge[];
} {
  const graph = new dagre.graphlib.Graph();
  graph.setGraph({ rankdir: "TB", nodesep: 24, ranksep: 48 });
  graph.setDefaultEdgeLabel(() => ({}));

  for (const step of plan.steps) {
    graph.setNode(step.step_id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }
  const ids = new Set(plan.steps.map((s) => s.step_id));
  const explicitEdges: Array<[string, string]> = [];
  for (const step of plan.steps) {
    for (const dep of step.depends_on) {
      if (ids.has(dep)) explicitEdges.push([dep, step.step_id]);
    }
  }
  // When the plan declares no dependencies, the steps would all land on a single
  // rank and render as a horizontal row. Chain them in plan order so the graph
  // reads top-to-bottom; these implicit edges are drawn dashed to set them apart
  // from declared dependencies. A real DAG (any declared edge) is rendered as-is.
  const implicit = explicitEdges.length === 0 && plan.steps.length > 1;
  const layoutEdges: Array<[string, string]> = implicit
    ? plan.steps
        .slice(1)
        .map((s, i) => [plan.steps[i].step_id, s.step_id] as [string, string])
    : explicitEdges;
  for (const [from, to] of layoutEdges) {
    graph.setEdge(from, to);
  }

  dagre.layout(graph);

  const nodes: PlanDagNode[] = plan.steps.map((step, i) => {
    const pos = graph.node(step.step_id);
    const x = pos ? pos.x - NODE_WIDTH / 2 : 0;
    const y = pos ? pos.y - NODE_HEIGHT / 2 : 0;
    return {
      id: step.step_id,
      type: "planStep",
      position: { x, y },
      data: { step, index: i + 1, removed: false },
    };
  });

  const graphHeight = (graph.graph().height ?? 0) as number;
  const tombstoneY = graphHeight + NODE_HEIGHT;
  const liveIds = new Set(plan.steps.map((s) => s.step_id));
  removed
    .filter((step) => !liveIds.has(step.step_id))
    .forEach((step, index) => {
      nodes.push({
        id: step.step_id,
        type: "planStep",
        position: { x: index * (NODE_WIDTH + 24), y: tombstoneY },
        data: { step, index: 0, removed: true },
      });
    });

  const stepById = new Map(plan.steps.map((s) => [s.step_id, s]));
  const edges: Edge[] = layoutEdges.map(([from, to]) => ({
    id: `${from}->${to}`,
    source: from,
    target: to,
    type: "smoothstep",
    animated: stepById.get(to)?.status === "in_progress",
    markerEnd: { type: MarkerType.ArrowClosed },
    ...(implicit ? { style: "stroke-dasharray: 5 5; opacity: 0.6;" } : {}),
  }));

  return { nodes, edges };
}
