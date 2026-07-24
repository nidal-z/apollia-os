import { describe, it, expect } from "vitest";
import {
  parseFileList,
  isFolder,
  humanSize,
  prettyJson,
  parseBashOutput,
  countOutputLines,
  basename,
  dirname,
  totalLines,
  parseFileGlob,
  parseGrep,
  parseHttp,
  httpStatusClass,
  parseMemory,
  parsePlanCall,
  parseTodos,
} from "./toolBodies";

describe("parseFileList", () => {
  it("parses a bare JSON array of entries", () => {
    // GIVEN a raw file_list output as a bare array
    const raw = JSON.stringify([
      { name: "apollia-runtime", type: "dir", entries: 48 },
      { name: "Cargo.toml", type: "file", size: 2148 },
    ]);

    // WHEN parsed
    const entries = parseFileList(raw);

    // THEN both entries are recovered with their metadata
    expect(entries).toHaveLength(2);
    expect(entries?.[0]).toMatchObject({ name: "apollia-runtime", type: "dir", entries: 48 });
    expect(entries?.[1]).toMatchObject({ name: "Cargo.toml", type: "file", size: 2148 });
  });

  it("unwraps an object that carries the array under a common key", () => {
    // GIVEN an object-wrapped listing
    const raw = JSON.stringify({ entries: [{ name: "README.md", type: "file", size: 10 }] });

    // WHEN parsed
    const entries = parseFileList(raw);

    // THEN the nested array is used
    expect(entries).toHaveLength(1);
    expect(entries?.[0].name).toBe("README.md");
  });

  it("takes the basename when an entry only carries a path", () => {
    // GIVEN an entry keyed by path
    const raw = JSON.stringify([{ path: "src/lib/chat/reasoning.ts", type: "file" }]);

    // WHEN parsed
    const entries = parseFileList(raw);

    // THEN only the final path component is shown
    expect(entries?.[0].name).toBe("reasoning.ts");
  });

  it("returns null on unparseable or empty output", () => {
    // GIVEN malformed and empty inputs
    // WHEN parsed / THEN both fall back to null
    expect(parseFileList("not json")).toBeNull();
    expect(parseFileList("")).toBeNull();
    expect(parseFileList("[]")).toBeNull();
  });
});

describe("isFolder", () => {
  it("recognizes dir and directory types", () => {
    expect(isFolder({ name: "a", type: "dir" })).toBe(true);
    expect(isFolder({ name: "a", type: "directory" })).toBe(true);
    expect(isFolder({ name: "a", type: "file" })).toBe(false);
  });
});

describe("humanSize", () => {
  it("keeps sub-kibibyte sizes in bytes", () => {
    expect(humanSize(512)).toEqual({ value: 512, unit: "bytes" });
  });

  it("rounds kibibytes and mebibytes to one decimal", () => {
    // GIVEN 2148 bytes and ~1.5 MiB
    // WHEN converted / THEN the magnitude and unit bucket are correct
    expect(humanSize(2148)).toEqual({ value: 2.1, unit: "kb" });
    expect(humanSize(1_572_864)).toEqual({ value: 1.5, unit: "mb" });
  });
});

describe("prettyJson", () => {
  it("indents JSON and passes plain text through untouched", () => {
    expect(prettyJson('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyJson("plain log line")).toBe("plain log line");
    expect(prettyJson(null)).toBe("");
  });
});

describe("parseBashOutput", () => {
  it("splits a JSON stdout/stderr/exit payload", () => {
    // GIVEN a structured bash output
    const raw = JSON.stringify({ stdout: "41\n", stderr: "", exit_code: 0 });

    // WHEN parsed
    const out = parseBashOutput(raw, null);

    // THEN the streams and exit code are recovered
    expect(out.body).toBe("41\n");
    expect(out.exitCode).toBe(0);
  });

  it("treats plain text as the body and uses the fallback exit code", () => {
    const out = parseBashOutput("some plain output", 2);
    expect(out.body).toBe("some plain output");
    expect(out.exitCode).toBe(2);
  });
});

describe("countOutputLines", () => {
  it("counts only non-empty lines", () => {
    expect(countOutputLines("a\n\nb\n")).toBe(2);
    expect(countOutputLines(null)).toBe(0);
  });
});

describe("basename", () => {
  it("returns the final path component for both separators", () => {
    expect(basename("/a/b/Cargo.toml")).toBe("Cargo.toml");
    expect(basename("C:\\x\\y\\file.txt")).toBe("file.txt");
  });
});

describe("dirname", () => {
  it("returns the parent path and empty for a bare name", () => {
    // GIVEN nested and bare paths
    // WHEN taking the directory portion
    // THEN the parent is returned, or "" when there is none
    expect(dirname("src/lib/chat/reasoning.ts")).toBe("src/lib/chat");
    expect(dirname("Cargo.toml")).toBe("");
  });
});

describe("totalLines", () => {
  it("counts every line including blanks, ignoring a trailing newline", () => {
    expect(totalLines("a\n\nb\n")).toBe(3);
    expect(totalLines("")).toBe(0);
    expect(totalLines(null)).toBe(0);
  });
});

describe("parseFileGlob", () => {
  it("recovers a matches array and a bare array alike", () => {
    // GIVEN a wrapped and a bare glob output
    const wrapped = JSON.stringify({ matches: ["src/a.rs", "src/b.rs"] });
    const bare = JSON.stringify(["only.rs"]);

    // WHEN parsed
    // THEN both yield the path list
    expect(parseFileGlob(wrapped)).toEqual(["src/a.rs", "src/b.rs"]);
    expect(parseFileGlob(bare)).toEqual(["only.rs"]);
  });

  it("returns null on empty or malformed output", () => {
    expect(parseFileGlob(JSON.stringify({ matches: [] }))).toBeNull();
    expect(parseFileGlob("not json")).toBeNull();
    expect(parseFileGlob(null)).toBeNull();
  });
});

describe("parseGrep", () => {
  it("groups matches by file and counts them", () => {
    // GIVEN a grep output with two matches in one file and one in another
    const raw = JSON.stringify({
      matches: [
        { file: "a.rs", line_number: 3, content: "fn main" },
        { file: "a.rs", line_number: 9, content: "let x" },
        { file: "b.rs", line_number: 1, content: "use std" },
      ],
      truncated: false,
      files_searched: 12,
    });

    // WHEN parsed
    const parsed = parseGrep(raw);

    // THEN matches are grouped by file with the counters preserved
    expect(parsed?.groups).toHaveLength(2);
    expect(parsed?.groups[0]).toEqual({
      file: "a.rs",
      rows: [
        { line: 3, text: "fn main" },
        { line: 9, text: "let x" },
      ],
    });
    expect(parsed?.totalMatches).toBe(3);
    expect(parsed?.filesSearched).toBe(12);
    expect(parsed?.truncated).toBe(false);
  });

  it("returns null when there are no usable matches", () => {
    expect(parseGrep(JSON.stringify({ matches: [] }))).toBeNull();
    expect(parseGrep("garbage")).toBeNull();
  });
});

describe("parseHttp / httpStatusClass", () => {
  it("recovers status, content-type and size case-insensitively", () => {
    // GIVEN an http_fetch output with mixed-case headers and no content-length
    const raw = JSON.stringify({
      status: 200,
      headers: { "Content-Type": "application/json; charset=utf-8" },
      body: "hello",
      duration_ms: 42,
    });

    // WHEN parsed
    const parsed = parseHttp(raw);

    // THEN status, trimmed content-type, and a body-derived byte size come back
    expect(parsed?.status).toBe(200);
    expect(parsed?.contentType).toBe("application/json");
    expect(parsed?.byteSize).toBe(5);
    expect(parsed?.durationMs).toBe(42);
  });

  it("returns null without a numeric status", () => {
    expect(parseHttp(JSON.stringify({ body: "x" }))).toBeNull();
    expect(parseHttp(null)).toBeNull();
  });

  it("buckets status codes into coarse classes", () => {
    expect(httpStatusClass(100)).toBe("info");
    expect(httpStatusClass(204)).toBe("success");
    expect(httpStatusClass(301)).toBe("redirect");
    expect(httpStatusClass(404)).toBe("client");
    expect(httpStatusClass(503)).toBe("server");
    expect(httpStatusClass(700)).toBe("unknown");
  });
});

describe("parseMemory", () => {
  it("recovers entries and the total-found counter", () => {
    // GIVEN a memory_search output
    const raw = JSON.stringify({
      results: [
        { score: 0.9, source: "semantic", content: "user likes tea", relevance: 0.8 },
        { score: 0.4, source: "episodic", content: "logged in yesterday" },
      ],
      total_found: 2,
    });

    // WHEN parsed
    const parsed = parseMemory(raw);

    // THEN both entries and the counter are recovered
    expect(parsed?.entries).toHaveLength(2);
    expect(parsed?.entries[0]).toMatchObject({
      content: "user likes tea",
      relevance: 0.8,
    });
    expect(parsed?.totalFound).toBe(2);
  });

  it("skips entries with no content and returns null when none remain", () => {
    const raw = JSON.stringify({ results: [{ score: 1, source: "s" }] });
    expect(parseMemory(raw)).toBeNull();
    expect(parseMemory("nope")).toBeNull();
  });
});

describe("parsePlanCall", () => {
  it("reads a propose call's step summaries from the output", () => {
    // GIVEN a plan_propose output snapshot
    const args = { steps: [{ step_id: "a" }, { step_id: "b" }] };
    const output = JSON.stringify({
      ok: true,
      steps: [
        { step_id: "a", title: "First", status: "pending" },
        { step_id: "b", title: "Second", status: "in_progress" },
      ],
    });

    // WHEN parsed
    const info = parsePlanCall(args, output);

    // THEN the propose count and output steps are recovered
    expect(info.ok).toBe(true);
    expect(info.proposeCount).toBe(2);
    expect(info.outputSteps).toHaveLength(2);
    expect(info.outputSteps?.[0]).toEqual({
      stepId: "a",
      title: "First",
      status: "pending",
    });
  });

  it("reads an add-step call's affected step from the args", () => {
    // GIVEN a plan_add_step call
    const args = { step: { step_id: "c", description: "Do the thing" } };
    const info = parsePlanCall(args, null);

    // THEN the arg step is normalized, falling back to description for the title
    expect(info.argStep).toEqual({
      stepId: "c",
      title: "Do the thing",
      status: null,
    });
  });

  it("flags a failed call and reads status changes", () => {
    // GIVEN a failed result and a set-status call
    expect(parsePlanCall({}, JSON.stringify({ ok: false })).ok).toBe(false);
    const info = parsePlanCall(
      { step_id: "a", status: "completed" },
      JSON.stringify({ ok: true }),
    );
    expect(info.stepId).toBe("a");
    expect(info.status).toBe("completed");
  });
});

describe("parseTodos", () => {
  it("recovers items from the args list", () => {
    // GIVEN a todo_write args payload
    const args = {
      items: [
        { id: "t1", content: "Read config", status: "completed" },
        { id: "t2", content: "Analyse logs", status: "in_progress" },
      ],
    };

    // WHEN parsed
    const items = parseTodos(args);

    // THEN every item is recovered with its status
    expect(items).toHaveLength(2);
    expect(items?.[1]).toEqual({
      id: "t2",
      content: "Analyse logs",
      status: "in_progress",
    });
  });

  it("returns null when there is no usable item", () => {
    expect(parseTodos({ items: [] })).toBeNull();
    expect(parseTodos({ items: [{ id: "x" }] })).toBeNull();
    expect(parseTodos({})).toBeNull();
  });
});
