import { describe, test, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  handleThinkingStarted,
  handleThinkingEnded,
  handleDecisionPointRecorded,
  latestThinking,
  latestDecisionPoint,
  latestDecisionTurnId,
  resetThinking,
} from "./thinking";
import type { DecisionPoint } from "$lib/types";

function decision(turnId: string): DecisionPoint {
  return {
    turn_id: turnId,
    kind: "tool_choice",
    chosen: "read_file",
    alternatives: [
      { label: "bash", rejected_reason: "too broad", confidence_delta: -0.2 },
    ],
  };
}

beforeEach(() => {
  resetThinking();
});

describe("latestThinking", () => {
  test("is null before any turn starts", () => {
    // GIVEN a fresh store
    // WHEN nothing has happened
    // THEN there is no thinking to surface
    expect(get(latestThinking)).toBeNull();
  });

  test("reports a live trace between ThinkingStarted and ThinkingEnded", () => {
    // GIVEN a turn that started thinking
    handleThinkingStarted({ turn_id: "t-1", ts_ms: 1000 });

    // WHEN reading the latest thinking
    const live = get(latestThinking);

    // THEN it is live with no body yet (raw_content lands on ThinkingEnded)
    expect(live).toEqual({ content: "", live: true });
  });

  test("settles to a frozen trace with content on ThinkingEnded", () => {
    // GIVEN a turn that started then ended thinking
    handleThinkingStarted({ turn_id: "t-1", ts_ms: 1000 });
    handleThinkingEnded({
      turn_id: "t-1",
      ts_ms: 1500,
      duration_ms: 500,
      raw_content: "weighed options",
      tokens: 12,
    });

    // WHEN reading the latest thinking
    const settled = get(latestThinking);

    // THEN it is frozen and carries the raw content
    expect(settled).toEqual({ content: "weighed options", live: false });
  });

  test("tracks the most recently started turn", () => {
    // GIVEN two turns, the second started last
    handleThinkingStarted({ turn_id: "t-1", ts_ms: 1000 });
    handleThinkingStarted({ turn_id: "t-2", ts_ms: 2000 });

    // WHEN reading the latest thinking
    const live = get(latestThinking);

    // THEN it reflects the second turn
    expect(live).toEqual({ content: "", live: true });
  });
});

describe("handleDecisionPointRecorded", () => {
  test("surfaces the recorded decision as the latest", () => {
    // GIVEN a decision point recorded for a turn
    handleDecisionPointRecorded({ point: decision("t-1") });

    // WHEN reading the latest decision
    const point = get(latestDecisionPoint);

    // THEN it carries the chosen path and alternatives
    expect(point?.turn_id).toBe("t-1");
    expect(point?.chosen).toBe("read_file");
    expect(point?.alternatives).toHaveLength(1);
    expect(get(latestDecisionTurnId)).toBe("t-1");
  });

  test("keeps the most recent decision when several land", () => {
    // GIVEN two recorded decisions
    handleDecisionPointRecorded({ point: decision("t-1") });
    handleDecisionPointRecorded({ point: decision("t-2") });

    // WHEN reading the latest decision
    // THEN it is the second one
    expect(get(latestDecisionPoint)?.turn_id).toBe("t-2");
  });

  test("ignores a malformed point with no turn id", () => {
    // GIVEN a point missing its turn id
    handleDecisionPointRecorded({
      point: { chosen: "x" } as unknown as DecisionPoint,
    });

    // WHEN reading the latest decision
    // THEN nothing was recorded
    expect(get(latestDecisionPoint)).toBeNull();
  });
});

describe("resetThinking", () => {
  test("clears thinking and decision artifacts", () => {
    // GIVEN a populated store
    handleThinkingStarted({ turn_id: "t-1", ts_ms: 1000 });
    handleDecisionPointRecorded({ point: decision("t-1") });

    // WHEN the panel tears down
    resetThinking();

    // THEN both surfaces are empty again
    expect(get(latestThinking)).toBeNull();
    expect(get(latestDecisionPoint)).toBeNull();
    expect(get(latestDecisionTurnId)).toBeNull();
  });
});
