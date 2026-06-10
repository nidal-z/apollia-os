// Pure layered auto-layout for the plan DAG.
//
// Turns the session plan (steps + `depends_on` edges) into positioned xyflow
// nodes and directed edges. Layout is top-to-bottom (`rankdir: "TB"`) so a step
// always sits below the steps it depends on. No DOM, no side effect: callable
// from a `$derived` and unit-testable in isolation.

import dagre from "dagre";
import { MarkerType, type Node, type Edge } from "@xyflow/svelte";
import type { SessionPlan, SessionPlanStep } from "$lib/stores/chatPlanMode";

/** Geometry of a node, in sync with the rendered card width/height. */
export const NODE_WIDTH = 220;
export const NODE_HEIGHT = 104;

/** Data carried by a plan-step node (read by `PlanStepNode`). */
export interface StepNodeData extends Record<string, unknown> {
  step: SessionPlanStep;
  /** Set when the step was dropped by a replan (tombstone rendering). */
  removed: boolean;
}

export type PlanDagNode = Node<StepNodeData, "planStep">;

/**
 * Computes the layered DAG layout for a plan.
 *
 * Returns positioned nodes (type `"planStep"`) and directed dependency edges.
 * An edge is `animated` when its target step is currently in progress.
 */
export function layoutPlan(plan: SessionPlan): {
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
  for (const step of plan.steps) {
    for (const dep of step.depends_on) {
      if (ids.has(dep)) {
        graph.setEdge(dep, step.step_id);
      }
    }
  }

  dagre.layout(graph);

  const nodes: PlanDagNode[] = plan.steps.map((step) => {
    const pos = graph.node(step.step_id);
    const x = pos ? pos.x - NODE_WIDTH / 2 : 0;
    const y = pos ? pos.y - NODE_HEIGHT / 2 : 0;
    return {
      id: step.step_id,
      type: "planStep",
      position: { x, y },
      data: { step, removed: false },
    };
  });

  const edges: Edge[] = plan.steps.flatMap((step) =>
    step.depends_on
      .filter((dep) => ids.has(dep))
      .map((dep) => ({
        id: `${dep}->${step.step_id}`,
        source: dep,
        target: step.step_id,
        type: "smoothstep",
        animated: step.status === "in_progress",
        markerEnd: { type: MarkerType.ArrowClosed },
      })),
  );

  return { nodes, edges };
}
