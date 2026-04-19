import { describe, test, expect } from "vitest";
import {
  parseConfidence,
  segmentize,
  resolveCitations,
  type Citation,
} from "./confidenceParser";

/**
 * Unit tests for the frontend confidence parser (US-SP42-046 — Pattern P10).
 * Mirrors the Rust test suite in `apollia-runtime::analyzers::confidence_parser`.
 */

describe("parseConfidence", () => {
  test("wraps a single high-confidence span and strips markers", () => {
    const p = parseConfidence("Hello [conf:high]world[/conf] !");
    expect(p.text).toBe("Hello world !");
    expect(p.assertions).toHaveLength(1);
    const a = p.assertions[0];
    expect(a.confidence).toBe("high");
    expect(p.text.slice(a.text_range.start, a.text_range.end)).toBe("world");
  });

  test("handles nested conf spans (inner closes first)", () => {
    const p = parseConfidence("[conf:low]A [conf:high]B[/conf] C[/conf]");
    expect(p.text).toBe("A B C");
    expect(p.assertions).toHaveLength(2);
    expect(p.assertions[0].confidence).toBe("high");
    expect(p.assertions[1].confidence).toBe("low");
    const inner = p.assertions[0];
    expect(p.text.slice(inner.text_range.start, inner.text_range.end)).toBe("B");
  });

  test("attaches inline citation ids to the enclosing span", () => {
    const p = parseConfidence(
      "Earth [conf:high]per NASA[cite:nasa-2024][/conf].",
    );
    expect(p.text).toBe("Earth per NASA.");
    expect(p.assertions[0].citation_ids).toEqual(["nasa-2024"]);
  });

  test("splits and dedupes comma-separated cite ids", () => {
    const p = parseConfidence("[conf:medium]x[cite:a, b, a][/conf]");
    expect(p.assertions[0].citation_ids).toEqual(["a", "b"]);
  });

  test("accepts the 'med' alias as medium", () => {
    const p = parseConfidence("[conf:med]x[/conf]");
    expect(p.assertions[0].confidence).toBe("medium");
  });

  test("leaves unmarked text unchanged", () => {
    const p = parseConfidence("plain prose, nothing special.");
    expect(p.text).toBe("plain prose, nothing special.");
    expect(p.assertions).toHaveLength(0);
  });

  test("preserves unknown confidence levels verbatim", () => {
    const p = parseConfidence("[conf:ultra]foo[/conf]");
    expect(p.text).toBe("[conf:ultra]foo[/conf]");
    expect(p.assertions).toHaveLength(0);
  });

  test("keeps orphan [/conf] as literal text", () => {
    const p = parseConfidence("foo[/conf] bar");
    expect(p.text).toBe("foo[/conf] bar");
    expect(p.assertions).toHaveLength(0);
  });

  test("drops assertion for an unclosed span but preserves text", () => {
    const p = parseConfidence("[conf:high]dangling");
    expect(p.text).toBe("dangling");
    expect(p.assertions).toHaveLength(0);
  });

  test("keeps orphan cite markers as literal", () => {
    const p = parseConfidence("see [cite:x]");
    expect(p.text).toBe("see [cite:x]");
    expect(p.assertions).toHaveLength(0);
  });

  test("passes through malformed brackets (missing closing ])", () => {
    const p = parseConfidence("an [unbounded marker");
    expect(p.text).toBe("an [unbounded marker");
  });

  test("accumulates multiple cites inside a single span", () => {
    const p = parseConfidence("[conf:high]A[cite:a] B[cite:b][/conf]");
    expect(p.text).toBe("A B");
    expect(p.assertions[0].citation_ids).toEqual(["a", "b"]);
  });
});

describe("segmentize", () => {
  test("returns a single plain segment when no assertions", () => {
    const segs = segmentize({ text: "hello", assertions: [] });
    expect(segs).toEqual([{ text: "hello", assertion: null }]);
  });

  test("splits text into prose + assertion segments", () => {
    const p = parseConfidence("prefix [conf:high]bold[/conf] suffix");
    const segs = segmentize(p);
    expect(segs.map((s) => s.text)).toEqual(["prefix ", "bold", " suffix"]);
    expect(segs[0].assertion).toBeNull();
    expect(segs[1].assertion).not.toBeNull();
    expect(segs[1].assertion?.confidence).toBe("high");
    expect(segs[2].assertion).toBeNull();
  });

  test("innermost assertion wins on nested spans", () => {
    const p = parseConfidence("[conf:low]A [conf:high]B[/conf] C[/conf]");
    const segs = segmentize(p);
    // "A " → low, "B" → high, " C" → low
    const byText = new Map(segs.map((s) => [s.text, s.assertion?.confidence] as const));
    expect(byText.get("A ")).toBe("low");
    expect(byText.get("B")).toBe("high");
    expect(byText.get(" C")).toBe("low");
  });
});

describe("resolveCitations", () => {
  const citations: Citation[] = [
    { id: "a", title: "A" },
    { id: "b", title: "B", url: "https://b.example" },
  ];

  test("resolves known ids preserving request order", () => {
    expect(resolveCitations(["b", "a"], citations).map((c) => c.id)).toEqual([
      "b",
      "a",
    ]);
  });

  test("drops unknown ids silently", () => {
    expect(resolveCitations(["a", "ghost"], citations).map((c) => c.id)).toEqual([
      "a",
    ]);
  });
});

describe("UI rendering snapshot (footnote numbering)", () => {
  test("footnote numbers follow first-reference order", () => {
    const p = parseConfidence(
      "[conf:high]first[cite:beta][/conf] and [conf:low]second[cite:alpha][cite:beta][/conf]",
    );
    // Replay the numbering logic used in MessageRenderer.
    const numbers = new Map<string, number>();
    let next = 1;
    for (const a of p.assertions) {
      for (const id of a.citation_ids) {
        if (!numbers.has(id)) numbers.set(id, next++);
      }
    }
    expect([...numbers.entries()]).toEqual([
      ["beta", 1],
      ["alpha", 2],
    ]);
  });
});
