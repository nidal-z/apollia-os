import { describe, test, expect } from "vitest";
import type { ReasoningItem } from "./reasoning";
import {
  formatThoughtDuration,
  shortPath,
  stepActionKey,
  stepParameters,
  stepState,
  stepTarget,
  TARGET_MAX,
} from "./toolStep";

type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

function call(tool: string, args: Record<string, unknown>): ToolCallItem {
  return { id: "m-tc-0", kind: "tool_call", status: "success", tool, args, output: null };
}

describe("tool step - state", () => {
  test("each reasoning status maps onto one of the four step states", () => {
    // GIVEN every status a step can carry
    const statuses = ["pending", "running", "approved", "success", "error", "rejected"] as const;

    // WHEN each is mapped
    const states = statuses.map(stepState);

    // THEN waiting reads as running, approval as success, refusal as its own state
    expect(states).toEqual(["running", "running", "success", "success", "error", "refused"]);
  });
});

describe("tool step - action name", () => {
  test("each tool with a body of its own has a plain action name", () => {
    // GIVEN the twelve tools with a refreshed body, edition included
    const tools = [
      "web_search", "web_read", "file_list", "file_read", "file_write", "file_edit",
      "file_glob", "file_grep", "bash_executor", "python_executor", "http_fetch",
      "memory_search", "todo_write",
    ];

    // WHEN their action key is looked up
    const keys = tools.map(stepActionKey);

    // THEN each has one, under the actions namespace
    expect(keys.every((k) => k?.startsWith("tools.actions."))).toBe(true);
  });

  test("a tool without a body of its own has no action key", () => {
    // GIVEN a connection tool
    // WHEN its action key is looked up
    // THEN there is none, and the step falls back to the tool's label
    expect(stepActionKey("gmail.send")).toBeNull();
  });
});

describe("tool step - target", () => {
  test("the target is the query, the host, the file or the pattern", () => {
    // GIVEN one call per family of argument
    const search = call("web_search", { query: "electricity rates 2026" });
    const read = call("web_read", { url: "https://www.service-public.fr/particuliers" });
    const file = call("file_read", { path: "Accounting/notes.md" });
    const glob = call("file_glob", { pattern: "**/*march*.xlsx" });
    const list = call("file_list", { dir: "Documents/Accounting/2026" });

    // WHEN each target is read
    const targets = [search, read, file, glob, list].map(stepTarget);

    // THEN each names the short thing the step was about
    expect(targets).toEqual([
      { text: "electricity rates 2026" },
      { text: "service-public.fr" },
      { text: "notes.md" },
      { text: "**/*march*.xlsx" },
      { text: "Accounting/2026" },
    ]);
  });

  test("Python counts its lines and the task list its progress", () => {
    // GIVEN a three-line script and a list with two tasks done out of three
    const python = call("python_executor", { code: "a = 1\nb = 2\nprint(a + b)\n" });
    const todo = call("todo_write", {
      items: [
        { id: "1", content: "Read", status: "completed" },
        { id: "2", content: "Filter", status: "completed" },
        { id: "3", content: "Add up", status: "pending" },
      ],
    });

    // WHEN their targets are read
    // THEN each is a catalogue sentence with its numbers
    expect(stepTarget(python)).toEqual({ key: "tools.body.code_lines", values: { n: 3 } });
    expect(stepTarget(todo)).toEqual({ key: "tools.body.done_of_total", values: { done: 2, total: 3 } });
  });

  test("a long command is cut, and a call with nothing to name has no target", () => {
    // GIVEN a very long one-line command, and a call with no argument
    const long = call("bash_executor", { command: "x".repeat(200) });
    const empty = call("gmail.list", {});

    // WHEN their targets are read
    const cut = stepTarget(long);

    // THEN the first stops at the limit with an ellipsis, the second is absent
    expect(cut && "text" in cut ? cut.text.length : 0).toBe(TARGET_MAX);
    expect(cut && "text" in cut ? cut.text.endsWith("…") : false).toBe(true);
    expect(stepTarget(empty)).toBeNull();
  });

  test("a path keeps its last two segments", () => {
    // GIVEN a deep path
    // WHEN it is shortened
    // THEN only the part a reader recognises stays
    expect(shortPath("/Users/a/Documents/Accounting/2026")).toBe("Accounting/2026");
  });
});

describe("tool step - thought duration", () => {
  test("seconds round to the second, never below one, minutes past sixty", () => {
    // GIVEN durations under a second, of a few seconds, and past a minute
    const values = [120, 4_300, 60_000, 65_400];

    // WHEN each is formatted
    const labels = values.map(formatThoughtDuration);

    // THEN each reads as a reader would say it
    expect(labels).toEqual(["1 s", "4 s", "1 min", "1 min 5 s"]);
  });
});

describe("tool step - generic parameters", () => {
  test("parameters are listed short, empty ones dropped, objects in one line", () => {
    // GIVEN arguments of every shape
    const args = { file: "march-invoices.xlsx", sheet: "March", empty: "", options: { header: true } };

    // WHEN they are listed
    const params = stepParameters(args);

    // THEN each readable one stays, in order, the empty one is gone
    expect(params).toEqual([
      ["file", "march-invoices.xlsx"],
      ["sheet", "March"],
      ["options", '{"header":true}'],
    ]);
  });
});
