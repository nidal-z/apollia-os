import { describe, it, expect } from "vitest";
import { fuzzyScore, fuzzyMatch } from "./fuzzy";

describe("fuzzyScore", () => {
  it("returns null when characters do not appear as a subsequence", () => {
    expect(fuzzyScore("dashboard", "xyz")).toBeNull();
  });

  it("scores prefix matches higher than mid-string matches", () => {
    const prefix = fuzzyScore("dashboard", "dash")!;
    const mid = fuzzyScore("my dashboard", "dash")!;
    expect(prefix).toBeGreaterThan(mid);
  });

  it("scores contiguous matches higher than scattered matches", () => {
    const contiguous = fuzzyScore("agents", "age")!;
    const scattered = fuzzyScore("analogue events", "age")!;
    expect(contiguous).toBeGreaterThan(scattered);
  });

  it("is case-insensitive", () => {
    expect(fuzzyScore("Dashboard", "dash")).toBeGreaterThan(0);
  });
});

describe("fuzzyMatch", () => {
  it("falls back to keywords when label does not match", () => {
    const score = fuzzyMatch("Start all agents", ["run"], "run");
    expect(score).not.toBeNull();
    expect(score!).toBeGreaterThan(0);
  });

  it("returns null when neither label nor keywords match", () => {
    expect(fuzzyMatch("foo", ["bar"], "zzz")).toBeNull();
  });

  it("prefers label match over keyword match when both match", () => {
    const labelMatch = fuzzyMatch("dashboard", ["home"], "dash")!;
    const keywordMatch = fuzzyMatch("home screen", ["dashboard"], "dash")!;
    expect(labelMatch).toBeGreaterThan(keywordMatch);
  });
});
