import { describe, test, expect, vi, beforeAll } from "vitest";
import { render } from "svelte/server";
import type { Component } from "svelte";
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

import WebSearchBody from "./WebSearchBody.svelte";
import WebReadBody from "./WebReadBody.svelte";
import FileListBody from "./FileListBody.svelte";
import FileReadBody from "./FileReadBody.svelte";
import FileWriteBody from "./FileWriteBody.svelte";
import FileGlobBody from "./FileGlobBody.svelte";
import FileGrepBody from "./FileGrepBody.svelte";
import BashBody from "./BashBody.svelte";
import PythonBody from "./PythonBody.svelte";
import HttpFetchBody from "./HttpFetchBody.svelte";
import MemorySearchBody from "./MemorySearchBody.svelte";
import TodoBody from "./TodoBody.svelte";
import GenericBody from "./GenericBody.svelte";
import type { ReasoningItem } from "$lib/chat/reasoning";

type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

beforeAll(async () => {
  locale.set("en");
  await waitLocale("en");
});

/** A finished call, as the runtime records it. */
function recorded(tool: string, args: Record<string, unknown>, output: unknown, duration_ms = 1_200): ToolCallItem {
  return {
    id: `m-tc-${tool}`,
    kind: "tool_call",
    status: "success",
    tool,
    args,
    output: typeof output === "string" ? output : JSON.stringify(output),
    duration_ms,
  };
}

function body(component: Component<{ item: ToolCallItem; sessionId?: string }>, item: ToolCallItem): string {
  // The server render escapes apostrophes; the assertions read plain text.
  return render(component, { props: { item, sessionId: "s-1" } }).body.replace(/&#39;/g, "'");
}

describe("tool bodies - each frame of the mockup, on a recorded call", () => {
  test("web search: the query, the count, each result by title and site", () => {
    // GIVEN a recorded search with two results
    const item = recorded("web_search", { query: "regulated electricity rates february 2026" }, {
      query: "regulated electricity rates february 2026",
      results: [
        { title: "Regulated sales rates", url: "https://www.cre.fr/tarifs", snippet: "", rank: 1 },
        { title: "What changes on your bill", url: "https://www.service-public.fr/a", snippet: "", rank: 2 },
      ],
      total_results: 2,
    });

    // WHEN its body renders
    const html = body(WebSearchBody, item);

    // THEN it reads as the frame: query, count, titles and sites
    expect(html).toContain("Search");
    expect(html).toContain("regulated electricity rates february 2026");
    expect(html).toContain("2 results");
    expect(html).toContain("Regulated sales rates");
    expect(html).toContain("cre.fr");
    expect(html).toContain("service-public.fr");
  });

  test("page read: the page title and address, the text read, the opening quoted", () => {
    // GIVEN a recorded page read
    const item = recorded("web_read", { url: "https://www.service-public.fr/particuliers/actualites" }, {
      url: "https://www.service-public.fr/particuliers/actualites",
      title: "What changes on your bill in 2026",
      content: "On February 1, 2026, the regulated rate drops by 15 %.",
      chars_total: 2400,
    });

    // WHEN its body renders
    const html = body(WebReadBody, item);

    // THEN title, address, amount read and quote are there
    expect(html).toContain("What changes on your bill in 2026");
    expect(html).toContain("service-public.fr/particuliers/actualites");
    expect(html).toContain("Text read: 2,400 characters");
    expect(html).toContain("“On February 1, 2026");
  });

  test("folder listing: the folder, then each entry with what it holds", () => {
    // GIVEN a recorded listing with a folder and a spreadsheet
    const item = recorded("file_list", { dir: "Documents/Accounting/2026" }, {
      entries: [
        { name: "March", type: "directory", entries: 12 },
        { name: "march-invoices.xlsx", type: "file", size: 49152 },
      ],
    });

    // WHEN its body renders
    const html = body(FileListBody, item);

    // THEN the folder heads the list, each entry with its count or size
    expect(html).toContain("Folder");
    expect(html).toContain("Documents/Accounting/2026");
    expect(html).toContain("March");
    expect(html).toContain("12 items");
    expect(html).toContain("march-invoices.xlsx");
    expect(html).toContain("48 KB");
  });

  test("file read: the file and its lines, then its content", () => {
    // GIVEN a recorded read of four lines
    const item = recorded("file_read", { path: "Accounting/notes.md" }, "# March notes\n- Chase Durand\n- Check the VAT\n- Close on April 5\n");

    // WHEN its body renders
    const html = body(FileReadBody, item);

    // THEN the file, the range and the lines show
    expect(html).toContain("Accounting/notes.md");
    expect(html).toContain("lines 1 to 4");
    expect(html).toContain("# March notes");
    expect(html).toContain("Close on April 5");
  });

  test("file edit: the file, what changed, the removed and added lines", () => {
    // GIVEN a recorded edit replacing one line
    const item = recorded(
      "file_edit",
      { path: "Accounting/notes.md", old_text: "- Chase Durand", new_text: "- Durand chased on April 2" },
      { replacements: 1 },
    );

    // WHEN its body renders
    const html = body(FileWriteBody, item);

    // THEN the summary counts one line each way, the diff shows both
    expect(html).toContain("1 line added, 1 line removed");
    expect(html).toContain("- - Chase Durand");
    expect(html).toContain("+ - Durand chased on April 2");
  });

  test("file write: the file, the lines written, the opening of the text", () => {
    // GIVEN a recorded write of two lines
    const item = recorded("file_write", { path: "report.md", content: "# Report\nTotal: 18,420 EUR\n" }, { ok: true });

    // WHEN its body renders
    const html = body(FileWriteBody, item);

    // THEN the count and the text show
    expect(html).toContain("report.md");
    expect(html).toContain("2 lines written");
    expect(html).toContain("Total: 18,420 EUR");
  });

  test("file search by name: the pattern, how many, each path", () => {
    // GIVEN a recorded search finding two files
    const item = recorded("file_glob", { pattern: "**/*march*.xlsx" }, {
      matches: ["Accounting/2026/march-invoices.xlsx", "Archives/2025/march-invoices.xlsx"],
    });

    // WHEN its body renders
    const html = body(FileGlobBody, item);

    // THEN it reads as the frame
    expect(html).toContain("Pattern");
    expect(html).toContain("**/*march*.xlsx");
    expect(html).toContain("2 files found");
    expect(html).toContain("Archives/2025/march-invoices.xlsx");
  });

  test("search in files: the words, how many passages in how many files, each passage", () => {
    // GIVEN a recorded search with two passages in one file
    const item = recorded("file_grep", { pattern: "Durand" }, {
      matches: [
        { file: "notes.md", line_number: 2, content: "- Chase Durand (invoice 2026-031)" },
        { file: "notes.md", line_number: 9, content: "Durand: deposit received on March 12" },
      ],
      files_searched: 14,
    });

    // WHEN its body renders
    const html = body(FileGrepBody, item);

    // THEN it reads as the frame
    expect(html).toContain("Words searched");
    expect(html).toContain("2 passages in 1 file");
    expect(html).toContain("notes.md, line 2");
    expect(html).toContain("Durand: deposit received on March 12");
  });

  test("command: the command and its output, then how it ended", () => {
    // GIVEN a recorded command that ended well in 0.1 s
    const item = recorded("bash_executor", { command: "wc -l exports/clients.csv" }, {
      stdout: "214 exports/clients.csv\n",
      stderr: "",
      exit_code: 0,
    }, 100);

    // WHEN its body renders
    const html = body(BashBody, item);

    // THEN the command, its output and the ending show
    expect(html).toContain("$ wc -l exports/clients.csv");
    expect(html).toContain("214 exports/clients.csv");
    expect(html).toContain("Finished, exit code 0, in 100 ms");
  });

  test("Python: the code, then its result", () => {
    // GIVEN a recorded run printing one number
    const item = recorded("python_executor", { code: "import csv\nprint(18420.0)\n" }, {
      stdout: "18420.0\n",
      stderr: "",
      exit_code: 0,
    });

    // WHEN its body renders
    const html = body(PythonBody, item);

    // THEN the code and the result show
    expect(html).toContain("import csv");
    expect(html).toContain("Result");
    expect(html).toContain("18420.0");
  });

  test("web address: the request, the response, the opening of what came back", () => {
    // GIVEN a recorded call answered 200
    const item = recorded("http_fetch", { url: "https://api.example.com/rate?currency=EUR", method: "get" }, {
      status: 200,
      headers: { "content-length": "1229" },
      body: '{"currency":"EUR","rate":1.0842}',
      duration_ms: 300,
    });

    // WHEN its body renders
    const html = body(HttpFetchBody, item);

    // THEN it reads as the frame
    expect(html).toContain("Request");
    expect(html).toContain("GET https://api.example.com/rate?currency=EUR");
    expect(html).toContain("Response 200, 1.2 KB, in 300 ms");
    expect(html).toContain(`"rate": 1.0842`);
  });

  test("memory search: what was looked for, each memory and the day it was kept", () => {
    // GIVEN a recorded search with two memories
    const item = recorded("memory_search", { query: "billing preferences" }, {
      results: [
        { score: 0.9, source: "profile", content: "Invoices go out on the 1st, as PDF.", created_at: "2026-02-14T09:00:00Z" },
        { score: 0.8, source: "profile", content: "Durand pays at 45 days.", created_at: "2026-03-03T09:00:00Z" },
      ],
      total_found: 2,
    });

    // WHEN its body renders
    const html = body(MemorySearchBody, item);

    // THEN it reads as the frame
    expect(html).toContain("billing preferences");
    expect(html).toContain("Invoices go out on the 1st, as PDF.");
    expect(html).toContain("remembered on February 14");
  });

  test("task list: how many are done, then each task", () => {
    // GIVEN a recorded list with two tasks done out of three
    const item = recorded("todo_write", {
      items: [
        { id: "1", content: "Read the invoice workbook", status: "completed" },
        { id: "2", content: "Filter the March invoices", status: "completed" },
        { id: "3", content: "Add up the amounts", status: "pending" },
      ],
    }, { count: 3 });

    // WHEN its body renders
    const html = body(TodoBody, item);

    // THEN it reads as the frame
    expect(html).toContain("2 of 3 done");
    expect(html).toContain("Add up the amounts");
  });

  test("generic: the parameters, then the result read as a sentence", () => {
    // GIVEN a recorded call of a tool with no body of its own
    const item = recorded("spreadsheet_read", { file: "march-invoices.xlsx", sheet_name: "March" }, {
      rows: [1, 2, 3],
    });

    // WHEN its body renders
    const html = body(GenericBody, item);

    // THEN the parameters and the result show, never the raw output
    expect(html).toContain("Parameters");
    expect(html).toContain("march-invoices.xlsx");
    expect(html).toContain("sheet name");
    expect(html).toContain("Result");
    expect(html).not.toContain("&quot;rows&quot;");
  });
});
