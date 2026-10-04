import { describe, test, expect, vi, beforeAll } from "vitest";
import { render } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import ToolStep from "./ToolStep.svelte";
import type { ReasoningItem, ReasoningStatus } from "$lib/chat/reasoning";

type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

beforeAll(async () => {
  locale.set("en");
  await waitLocale("en");
});

function search(status: ReasoningStatus, output: string | null, duration_ms: number | null): ToolCallItem {
  return {
    id: "m-tc-0",
    kind: "tool_call",
    status,
    tool: "web_search",
    args: { query: "electricity rates 2026" },
    output,
    duration_ms,
  };
}

function step(item: ToolCallItem, expanded = false, skin: "builder" | "operator" = "operator"): string {
  return render(ToolStep, { props: { item, skin, expanded, ontoggle: () => {} } }).body;
}

const RESULTS = JSON.stringify({
  query: "electricity rates 2026",
  results: [{ title: "Regulated sales rates", url: "https://www.cre.fr/tarifs", snippet: "", rank: 1 }],
  total_results: 1,
});

describe("tool step - one foldable line per state", () => {
  test("running: the action, its target and a working indicator, no failure", () => {
    // GIVEN a search still running
    // WHEN its line renders
    const html = step(search("running", null, null));

    // THEN it names the action and the target, shows it working, and nothing failed
    expect(html).toContain("Web search");
    expect(html).toContain("electricity rates 2026");
    expect(html).toContain('data-state="running"');
    expect(html).toContain('role="status"');
    expect(html).not.toContain("tool-step-failure");
  });

  test("succeeded: the action, the target, a check and the duration", () => {
    // GIVEN a search that ended in 1.2 s
    // WHEN its line renders
    const html = step(search("success", RESULTS, 1_200));

    // THEN the line carries the duration, in the reader's notation
    expect(html).toContain('data-state="success"');
    expect(html).toContain("1.2 s");
    expect(html).not.toContain("tool-step-failure");
  });

  test("failed: the reason shows under the line, even folded", () => {
    // GIVEN an edit that failed on a locked file
    const item: ToolCallItem = {
      id: "m-tc-1",
      kind: "tool_call",
      status: "error",
      tool: "file_edit",
      args: { path: "notes.md", old_text: "a", new_text: "b" },
      output: "tool error: io: the file is open in another application",
      duration_ms: 400,
    };

    // WHEN its folded line renders
    const html = step(item);

    // THEN it names the action and file, and says why it failed in plain words
    expect(html).toContain("Edit a file");
    expect(html).toContain("notes.md");
    expect(html).toContain('data-state="error"');
    expect(html).toContain("Failed: the file is open in another application");
    expect(html).not.toContain("tool error");
  });

  test("folded, the body is absent; unfolded, it holds the tool's own body", () => {
    // GIVEN the finished search
    const item = search("success", RESULTS, 1_200);

    // WHEN it renders folded, then unfolded
    const folded = step(item, false);
    const unfolded = step(item, true);

    // THEN only the unfolded line carries the search body
    expect(folded).not.toContain("tool-step-body");
    expect(unfolded).toContain("tool-step-body");
    expect(unfolded).toContain("tool-body-web_search");
    expect(unfolded).toContain("Regulated sales rates");
  });
});

describe("tool step - the developer mode adds raw data, never replaces the body", () => {
  test("off: the readable body alone, no raw data and no tool identifier", () => {
    // GIVEN the unfolded search, developer mode off
    // WHEN it renders
    const html = step(search("success", RESULTS, 1_200), true, "operator");

    // THEN the body shows, the raw data and the identifier do not
    expect(html).toContain("tool-body-web_search");
    expect(html).not.toContain("tool-step-raw");
    expect(html).not.toContain("web_search<");
  });

  test("on: the same body, then the raw data under the tool identifier", () => {
    // GIVEN the same search, developer mode on
    // WHEN it renders
    const html = step(search("success", RESULTS, 1_200), true, "builder");

    // THEN the body stays and the raw data follows it
    const body = html.indexOf("tool-body-web_search");
    const raw = html.indexOf("tool-step-raw");
    expect(body).toBeGreaterThan(-1);
    expect(raw).toBeGreaterThan(body);
    expect(html).toContain("Raw data");
  });
});
