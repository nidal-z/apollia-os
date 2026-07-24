import { describe, it, expect } from "vitest";
import {
  parseFileList,
  isFolder,
  humanSize,
  prettyJson,
  parseBashOutput,
  countOutputLines,
  basename,
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
