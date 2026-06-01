import { describe, it, expect } from "vitest";
import { detectSlashPrefix, filterCommands, SLASH_COMMANDS } from "./slashCommands";

describe("detectSlashPrefix", () => {
  it("returns null when input does not start with /", () => {
    expect(detectSlashPrefix("hello")).toBeNull();
    expect(detectSlashPrefix("")).toBeNull();
  });

  it("returns the token after a leading slash", () => {
    expect(detectSlashPrefix("/clear")).toBe("clear");
    expect(detectSlashPrefix("/exp")).toBe("exp");
    expect(detectSlashPrefix("/")).toBe("");
  });

  it("tolerates leading whitespace", () => {
    expect(detectSlashPrefix("  /rename foo")).toBe("rename");
  });

  it("stops at the first whitespace or newline", () => {
    expect(detectSlashPrefix("/export now")).toBe("export");
    expect(detectSlashPrefix("/clear\nnext")).toBe("clear");
  });
});

describe("filterCommands", () => {
  it("returns all commands for an empty prefix", () => {
    expect(filterCommands("")).toHaveLength(SLASH_COMMANDS.length);
  });

  it("filters by prefix", () => {
    const hits = filterCommands("ex");
    expect(hits.map((c) => c.id)).toEqual(["export"]);
  });

  it("is case-insensitive", () => {
    // `clear` n'existe plus dans SLASH_COMMANDS - on teste case-insensitivity
    // sur `export` (idem `ex` lowercase plus haut, mais en majuscules).
    expect(filterCommands("EX").map((c) => c.id)).toEqual(["export"]);
  });
});
