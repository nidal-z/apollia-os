import { describe, it, expect } from "vitest";
import { slugify } from "./slugify";

describe("slugify", () => {
  it("converts a typical label with diacritics", () => {
    expect(slugify("Alertes Slack équipe")).toBe("alertes-slack-equipe");
  });

  it("strips diacritics on a single repeating accent", () => {
    expect(slugify("ééé")).toBe("eee");
  });

  it("trims leading and trailing whitespace into trimmed slug", () => {
    expect(slugify("  Foo Bar  ")).toBe("foo-bar");
  });

  it("collapses consecutive separators into a single dash", () => {
    expect(slugify("test--mult")).toBe("test-mult");
    expect(slugify("a  b  c")).toBe("a-b-c");
  });

  it("strips non-alphanumeric punctuation", () => {
    expect(slugify("Slack: équipe #ops!")).toBe("slack-equipe-ops");
  });

  it("returns an empty string for input with no usable characters", () => {
    expect(slugify("")).toBe("");
    expect(slugify("   ")).toBe("");
    expect(slugify("!!!")).toBe("");
  });

  it("preserves digits", () => {
    expect(slugify("Canal 42")).toBe("canal-42");
  });

  it("lower-cases ASCII uppercase letters", () => {
    expect(slugify("BUREAU")).toBe("bureau");
  });

  it("handles a single character input (boundary)", () => {
    expect(slugify("A")).toBe("a");
    expect(slugify("9")).toBe("9");
  });

  it("does not produce leading or trailing dashes after punctuation", () => {
    expect(slugify("--Slack--")).toBe("slack");
    expect(slugify("...Bureau...")).toBe("bureau");
  });
});
