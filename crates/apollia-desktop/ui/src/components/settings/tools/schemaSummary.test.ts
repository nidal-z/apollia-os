import { describe, it, expect } from "vitest";
import {
  flattenSchema,
  hasSchema,
  prettyJson,
  typeLabel,
} from "./schemaSummary";

describe("typeLabel", () => {
  it("labels an array by its item type", () => {
    // GIVEN an array schema of strings
    const schema = { type: "array", items: { type: "string" } };

    // WHEN the label is resolved
    const label = typeLabel(schema);

    // THEN the item type carries the array suffix
    expect(label).toBe("string[]");
  });

  it("joins a union declared through anyOf", () => {
    // GIVEN a nullable string declared as anyOf
    const schema = { anyOf: [{ type: "string" }, { type: "null" }] };

    // WHEN the label is resolved
    const label = typeLabel(schema);

    // THEN both branches appear
    expect(label).toBe("string | null");
  });

  it("falls back to any on an unreadable node", () => {
    // GIVEN a node that is not an object
    const schema = 42;

    // WHEN the label is resolved
    const label = typeLabel(schema);

    // THEN the neutral label is used
    expect(label).toBe("any");
  });
});

describe("flattenSchema", () => {
  it("flattens properties with required, description, enum and default", () => {
    // GIVEN a tool input schema
    const schema = {
      type: "object",
      required: ["query"],
      properties: {
        query: { type: "string", description: "Search terms" },
        depth: { type: "integer", default: 3 },
        mode: { type: "string", enum: ["fast", "deep"] },
      },
    };

    // WHEN it is flattened
    const fields = flattenSchema(schema);

    // THEN every property becomes a row carrying its metadata
    expect(fields.map((f) => f.path)).toEqual(["query", "depth", "mode"]);
    expect(fields[0].required).toBe(true);
    expect(fields[0].description).toBe("Search terms");
    expect(fields[1].required).toBe(false);
    expect(fields[1].defaultValue).toBe("3");
    expect(fields[2].enumValues).toEqual(["fast", "deep"]);
  });

  it("descends into a nested object with a dotted path", () => {
    // GIVEN a schema with a nested object property
    const schema = {
      type: "object",
      properties: {
        options: {
          type: "object",
          required: ["timeout"],
          properties: { timeout: { type: "integer" } },
        },
      },
    };

    // WHEN it is flattened
    const fields = flattenSchema(schema);

    // THEN the child is indented and keyed by its dotted path
    expect(fields.map((f) => f.path)).toEqual(["options", "options.timeout"]);
    expect(fields[1].depth).toBe(1);
    expect(fields[1].required).toBe(true);
  });

  it("descends into object-shaped array items", () => {
    // GIVEN an array of objects
    const schema = {
      type: "object",
      properties: {
        edits: {
          type: "array",
          items: { type: "object", properties: { old: { type: "string" } } },
        },
      },
    };

    // WHEN it is flattened
    const fields = flattenSchema(schema);

    // THEN the item property is exposed under an array path
    expect(fields.map((f) => f.path)).toEqual(["edits", "edits[].old"]);
  });

  it("returns nothing for a document without properties", () => {
    // GIVEN a schema the walker cannot read as an object
    const schema = { type: "string" };

    // WHEN it is flattened
    const fields = flattenSchema(schema);

    // THEN the caller is told to fall back to the raw view
    expect(fields).toEqual([]);
  });
});

describe("hasSchema", () => {
  it("rejects null, undefined and the empty object", () => {
    // GIVEN the three empty forms a descriptor can carry
    // WHEN each is probed
    // THEN none counts as a schema
    expect(hasSchema(null)).toBe(false);
    expect(hasSchema(undefined)).toBe(false);
    expect(hasSchema({})).toBe(false);
  });

  it("accepts a populated document", () => {
    // GIVEN a non-empty schema
    const schema = { type: "object" };

    // WHEN it is probed
    // THEN it counts as a schema
    expect(hasSchema(schema)).toBe(true);
  });
});

describe("prettyJson", () => {
  it("indents the document", () => {
    // GIVEN a small schema
    const schema = { type: "object" };

    // WHEN it is pretty-printed
    const text = prettyJson(schema);

    // THEN the output is indented JSON
    expect(text).toBe('{\n  "type": "object"\n}');
  });

  it("returns an empty string on a cyclic value", () => {
    // GIVEN a cyclic object
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;

    // WHEN it is pretty-printed
    const text = prettyJson(cyclic);

    // THEN the failure is absorbed
    expect(text).toBe("");
  });
});
