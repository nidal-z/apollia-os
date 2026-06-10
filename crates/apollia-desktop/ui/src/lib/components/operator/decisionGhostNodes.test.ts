import { describe, it, expect } from "vitest";
import { toGhostNodes } from "./decisionGhostNodes";
import type { DecisionPoint } from "$lib/types";

const point: DecisionPoint = {
  turn_id: "t1",
  kind: "tool_choice",
  chosen: "web_read",
  alternatives: [
    { label: "web_search", rejected_reason: "cost too high", confidence_delta: -0.2 },
    { label: "ask_user", rejected_reason: "info already available", confidence_delta: -0.5 },
  ],
};

describe("toGhostNodes", () => {
  it("produces one descriptor per rejected alternative, anchored on the turn", () => {
    // GIVEN a decision point with two alternatives
    // WHEN computing the ghost nodes
    // THEN two descriptors are produced, anchored on the turn
    const ghosts = toGhostNodes(point);
    expect(ghosts).toHaveLength(2);
    expect(ghosts[0].anchorTurnId).toBe("t1");
    expect(ghosts[0].id).toBe("t1-ghost-0");
    expect(ghosts[0].label).toBe("web_search");
    expect(ghosts[0].rejectedReason).toBe("cost too high");
    expect(ghosts[0].confidenceDelta).toBe(-0.2);
  });

  it("returns an empty array without a decision point (empty case)", () => {
    // GIVEN no decision point
    // WHEN computing the ghost nodes
    // THEN the array is empty, nothing extra is rendered
    expect(toGhostNodes(null)).toEqual([]);
  });

  it("returns an empty array when there is no alternative (error / partial case)", () => {
    // GIVEN a decision point without rejected alternatives
    // WHEN computing the ghost nodes
    // THEN the array is empty
    expect(toGhostNodes({ ...point, alternatives: [] })).toEqual([]);
  });
});
