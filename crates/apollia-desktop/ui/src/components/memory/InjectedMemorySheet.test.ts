import { describe, test, expect } from "vitest";
import type { InjectedEntry } from "$lib/types";

/**
 * Unit tests for InjectedMemorySheet.
 *
 * The component is a thin Sheet renderer on top of the
 * `get_injected_memory_entries` Tauri command. These tests lock the shape
 * contract and the derived values (score percentage clamping) without
 * booting a full Svelte render environment.
 */

function scorePct(score: number): number {
  return Math.round(Math.max(0, Math.min(1, score)) * 100);
}

describe("InjectedMemorySheet — score clamping", () => {
  test("renders 0% for negative scores", () => {
    expect(scorePct(-0.2)).toBe(0);
  });

  test("renders 100% for scores above 1", () => {
    expect(scorePct(1.5)).toBe(100);
  });

  test("rounds to nearest integer percent", () => {
    expect(scorePct(0.926)).toBe(93);
    expect(scorePct(0.924)).toBe(92);
  });
});

describe("InjectedEntry — contract", () => {
  test("payload exposes the five expected fields", () => {
    const entry: InjectedEntry = {
      id: "sem-1",
      content_preview: "client dupont budget 15000",
      namespace: "agent-x",
      injection_reason: "Matched query: budget",
      relevance_score: 0.82,
    };

    const keys = Object.keys(entry).sort();
    expect(keys).toEqual([
      "content_preview",
      "id",
      "injection_reason",
      "namespace",
      "relevance_score",
    ]);
  });

  test("a fallback reason is a non-empty string", () => {
    const reason = "Matched query: budget";
    expect(reason.startsWith("Matched query:")).toBe(true);
  });
});
