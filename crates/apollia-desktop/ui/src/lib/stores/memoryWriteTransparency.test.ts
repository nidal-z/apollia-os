/**
 * Tests for memory-write transparency helpers.
 *
 * `recordRejectedInsight` must move an entry from the pending store into
 * the rejected store together with an explicit reason and a timestamp,
 * without losing the `source_quote` / `extraction_reasoning` fields.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  extractedInsights,
  rejectedInsights,
  recordRejectedInsight,
} from "./chat";
import type { InsightEntry } from "$lib/types";

const insight: InsightEntry = {
  id: "i-1",
  text: "prefers espresso",
  category: "preference",
  confidence: 0.82,
  source: "chat",
  source_quote: "I always start the day with an espresso.",
  extraction_reasoning: "Stable habit mentioned in first person.",
};

describe("recordRejectedInsight", () => {
  beforeEach(() => {
    extractedInsights.set([insight]);
    rejectedInsights.set([]);
  });

  // GIVEN a pending insight and a reject reason
  // WHEN recordRejectedInsight is called
  // THEN the entry moves to rejectedInsights with the reason preserved
  it("moves a pending insight into rejectedInsights with a reason", () => {
    recordRejectedInsight(insight, "Not a real preference, just one-off.");

    expect(get(extractedInsights)).toHaveLength(0);
    const rejected = get(rejectedInsights);
    expect(rejected).toHaveLength(1);
    expect(rejected[0].id).toBe("i-1");
    expect(rejected[0].rejected_reason).toBe(
      "Not a real preference, just one-off.",
    );
    expect(rejected[0].source_quote).toBe(insight.source_quote);
    expect(rejected[0].extraction_reasoning).toBe(insight.extraction_reasoning);
    expect(() => new Date(rejected[0].rejected_at).toISOString()).not.toThrow();
  });

  // GIVEN two rejections in sequence
  // WHEN each one lands in the store
  // THEN the most recent rejection sits at the head of the list
  it("prepends new rejections so the latest appears first", () => {
    const second: InsightEntry = { ...insight, id: "i-2", text: "likes dark mode" };
    extractedInsights.set([insight, second]);

    recordRejectedInsight(insight, "first reason");
    recordRejectedInsight(second, "second reason");

    const rejected = get(rejectedInsights);
    expect(rejected.map((e) => e.id)).toEqual(["i-2", "i-1"]);
  });
});
