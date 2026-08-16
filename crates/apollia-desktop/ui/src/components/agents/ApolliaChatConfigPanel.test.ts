import { describe, test, expect } from "vitest";
import { selectAllTools, type NamedTool } from "./ApolliaChatConfigPanel.svelte";

// DOM rendering is exercised by the Playwright layer (vitest runs in `node`).
// The button labelled "Select all" sits above a list the search box filters, so
// the question these tests answer is what happens to the tools the filter hides.

function tools(...names: string[]): NamedTool[] {
  return names.map((name) => ({ name }));
}

const TEN = tools(
  "fs.read",
  "fs.write",
  "http.fetch",
  "shell.run",
  "python.exec",
  "mail.send",
  "cal.list",
  "notes.append",
  "git.status",
  "db.query",
);

describe("ApolliaChatConfigPanel - selectAllTools", () => {
  test("a filter that hides seven of ten selections unchecks none of them", () => {
    // GIVEN ten tools already allowed, and a filter matching only three of them
    const current = new Set(TEN.map((tool) => tool.name));
    const listed = tools("fs.read", "fs.write", "git.status");

    // WHEN the user clicks "Select all" with that filter active
    const next = selectAllTools(current, listed);

    // THEN every tool that was checked is still checked
    for (const tool of TEN) {
      expect(next.has(tool.name)).toBe(true);
    }
    expect(next.size).toBe(10);
  });

  test("the filtered results are added to the selection", () => {
    // GIVEN one tool allowed, and a filter matching three others
    const current = new Set(["fs.read"]);
    const listed = tools("git.status", "db.query", "cal.list");

    // WHEN the user clicks "Select all"
    const next = selectAllTools(current, listed);

    // THEN the old selection and the filter results are both in
    expect([...next].sort()).toEqual([
      "cal.list",
      "db.query",
      "fs.read",
      "git.status",
    ]);
  });

  test("with no filter it still selects the whole list", () => {
    // GIVEN nothing allowed and an unfiltered list
    const current = new Set<string>();

    // WHEN the user clicks "Select all"
    const next = selectAllTools(current, TEN);

    // THEN every tool is allowed
    expect(next.size).toBe(TEN.length);
  });

  test("it never mutates the selection it was handed", () => {
    // GIVEN a selection the component still holds
    const current = new Set(["fs.read"]);

    // WHEN computing the next selection
    selectAllTools(current, tools("git.status"));

    // THEN the previous one is untouched, so the assignment is what changes state
    expect([...current]).toEqual(["fs.read"]);
  });

  test("an empty result set leaves the selection exactly as it was", () => {
    // GIVEN a selection and a filter that matches nothing
    const current = new Set(["fs.read", "fs.write"]);

    // WHEN the user clicks "Select all"
    const next = selectAllTools(current, []);

    // THEN nothing was added and nothing was removed
    expect([...next].sort()).toEqual(["fs.read", "fs.write"]);
  });
});
