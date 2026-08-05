import { describe, it, expect } from "vitest";
import { countPurgeMatches } from "./purgePreview";
import type { MemoryEntry } from "$lib/types";

const NOW = Date.parse("2026-08-05T12:00:00Z");

function entry(
  id: string,
  entryType: MemoryEntry["entry_type"],
  createdAt: string,
): MemoryEntry {
  return {
    id,
    entry_type: entryType,
    key: "k",
    value: "v",
    created_at: createdAt,
    expires_at: null,
    score: null,
  };
}

describe("countPurgeMatches", () => {
  it("counts only the entries older than the cutoff", () => {
    // GIVEN one entry from 40 days ago and one from yesterday
    const entries = [
      entry("old", "episodic", "2026-06-26T12:00:00Z"),
      entry("fresh", "episodic", "2026-08-04T12:00:00Z"),
    ];

    // WHEN a 30 day purge is previewed
    const count = countPurgeMatches(entries, "all", 30, NOW);

    // THEN only the older entry is counted
    expect(count).toBe(1);
  });

  it("restricts the count to the selected memory type", () => {
    // GIVEN three old entries, one per memory family
    const entries = [
      entry("e", "episodic", "2026-01-01T00:00:00Z"),
      entry("s", "semantic", "2026-01-01T00:00:00Z"),
      entry("p", "procedural", "2026-01-01T00:00:00Z"),
    ];

    // WHEN the preview targets the semantic family only
    const count = countPurgeMatches(entries, "semantic", 30, NOW);

    // THEN the other two families are left out
    expect(count).toBe(1);
  });

  it("takes everything when the age threshold is zero", () => {
    // GIVEN two entries, the most recent of them one second old
    const entries = [
      entry("old", "episodic", "2026-01-01T00:00:00Z"),
      entry("recent", "semantic", "2026-08-05T11:59:59Z"),
    ];

    // WHEN the threshold is 0 days
    const count = countPurgeMatches(entries, "all", 0, NOW);

    // THEN both are counted
    expect(count).toBe(2);
  });

  it("ignores entries whose creation date cannot be parsed", () => {
    // GIVEN an entry with a corrupt timestamp
    const entries = [entry("broken", "episodic", "not-a-date")];

    // WHEN the preview runs
    const count = countPurgeMatches(entries, "all", 0, NOW);

    // THEN it is not counted, since its age cannot be established
    expect(count).toBe(0);
  });

  it("returns zero on an empty namespace", () => {
    // GIVEN no entries at all
    const entries: MemoryEntry[] = [];

    // WHEN the preview runs
    const count = countPurgeMatches(entries, "all", 30, NOW);

    // THEN the preview reports nothing to lose
    expect(count).toBe(0);
  });
});
