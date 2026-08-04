import { describe, test, expect } from "vitest";
import { contextGaugeLabel, contextPct } from "./contextGauge";

describe("contextGaugeLabel", () => {
  test("renders unknown as --, never as a fake percentage", () => {
    // GIVEN a backend that reports no context window (Agent mode, or an
    // engine that omits usage)
    // WHEN the label is derived
    // THEN it reads "--": zero is an absent measurement, not an empty context
    expect(contextGaugeLabel(0, 0)).toBe("--");
    expect(contextGaugeLabel(0, 500)).toBe("--");
  });

  test("renders a real measurement as a rounded percentage", () => {
    // GIVEN a 32768-token window half full
    expect(contextGaugeLabel(32768, 16384)).toBe("50%");
    // AND an overfull report clamps at 100
    expect(contextGaugeLabel(100, 250)).toBe("100%");
  });
});

describe("contextPct", () => {
  test("clamps to 100 and zeroes on unknown window", () => {
    expect(contextPct(0, 500)).toBe(0);
    expect(contextPct(200, 100)).toBe(50);
    expect(contextPct(100, 250)).toBe(100);
  });
});
