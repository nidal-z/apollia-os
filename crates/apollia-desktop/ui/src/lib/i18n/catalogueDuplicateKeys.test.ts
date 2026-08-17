import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  CatalogueSyntaxError,
  describeDuplicateKeys,
  findDuplicateKeys,
} from "./catalogueDuplicateKeys";

const CATALOGUE_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));

function readCatalogue(name: string): string {
  return readFileSync(path.join(CATALOGUE_DIRECTORY, name), "utf-8");
}

describe("findDuplicateKeys", () => {
  test("a key declared twice at the root is reported with both lines", () => {
    // GIVEN a document whose root object declares "connections" twice
    const source = '{\n  "connections": {\n    "card": "a"\n  },\n  "connections": {\n    "card": "b"\n  }\n}\n';

    // WHEN the scanner reads it
    const duplicates = findDuplicateKeys(source);

    // THEN the key is named once, with its count and both declaring lines
    expect(duplicates).toEqual([{ path: "connections", count: 2, lines: [2, 5] }]);
  });

  test("a key declared twice inside a nested object carries its dotted path", () => {
    // GIVEN a duplicate two levels down
    const source = '{\n  "settings": {\n    "title": "a",\n    "title": "b"\n  }\n}\n';

    // WHEN the scanner reads it
    const duplicates = findDuplicateKeys(source);

    // THEN the path locates the key from the root
    expect(duplicates).toEqual([{ path: "settings.title", count: 2, lines: [3, 4] }]);
  });

  test("the same key in two sibling objects is not a duplicate", () => {
    // GIVEN two distinct objects that both declare "card"
    const source = '{\n  "a": { "card": "x" },\n  "b": { "card": "y" }\n}\n';

    // WHEN the scanner reads it
    const duplicates = findDuplicateKeys(source);

    // THEN nothing is reported, the constraint is per object
    expect(duplicates).toEqual([]);
  });

  test("a key-shaped literal inside a string value is not counted", () => {
    // GIVEN a value that contains a quoted key and a brace
    const source = '{\n  "a": "\\"a\\": {",\n  "b": "}"\n}\n';

    // WHEN the scanner reads it
    const duplicates = findDuplicateKeys(source);

    // THEN the string is consumed as one token and nothing is reported
    expect(duplicates).toEqual([]);
  });

  test("objects nested in an array are scanned too", () => {
    // GIVEN a duplicate inside the second element of an array
    const source = '{\n  "list": [\n    { "k": 1 },\n    {\n      "k": 1,\n      "k": 2\n    }\n  ]\n}\n';

    // WHEN the scanner reads it
    const duplicates = findDuplicateKeys(source);

    // THEN the element index appears in the path
    expect(duplicates).toEqual([{ path: "list[1].k", count: 2, lines: [5, 6] }]);
  });

  test("a key declared three times reports three lines", () => {
    // GIVEN a key repeated three times
    const source = '{\n  "k": 1,\n  "k": 2,\n  "k": 3\n}\n';

    // WHEN the scanner reads it
    const duplicates = findDuplicateKeys(source);

    // THEN the count and the lines both say three
    expect(duplicates).toEqual([{ path: "k", count: 3, lines: [2, 3, 4] }]);
  });

  test("malformed JSON fails rather than reporting zero duplicate", () => {
    // GIVEN a truncated document
    const source = '{\n  "a": {\n';

    // WHEN the scanner reads it
    // THEN it refuses, so a broken catalogue is never read as a clean one
    expect(() => findDuplicateKeys(source)).toThrow(CatalogueSyntaxError);
  });

  test("a document that JSON.parse accepts is accepted too", () => {
    // GIVEN every value shape the catalogues can carry
    const source =
      '{\n  "s": "text \\u00e9 \\n",\n  "n": -12.5e3,\n  "b": true,\n  "z": null,\n  "a": [],\n  "o": {}\n}\n';

    // WHEN both readers take it
    const duplicates = findDuplicateKeys(source);

    // THEN the scanner agrees with the parser, and finds nothing
    expect(() => JSON.parse(source)).not.toThrow();
    expect(duplicates).toEqual([]);
  });
});

describe("describeDuplicateKeys", () => {
  test("the message names the key, the count and the lines", () => {
    // GIVEN one duplicate
    const duplicates = [{ path: "connections", count: 2, lines: [352, 5411] }];

    // WHEN it is rendered
    const message = describeDuplicateKeys("fr.json", duplicates);

    // THEN the reader gets the key and the two places to look at
    expect(message).toBe(
      "fr.json: 1 duplicate key(s)\n  connections appears 2 times, lines 352, 5411",
    );
  });

  test("no duplicate renders as an explicit absence", () => {
    // GIVEN no duplicate
    // WHEN it is rendered
    const message = describeDuplicateKeys("en.json", []);

    // THEN the message says so rather than staying empty
    expect(message).toBe("en.json: no duplicate key");
  });
});

describe("the two catalogues", () => {
  for (const name of ["en.json", "fr.json"] as const) {
    test(`${name} declares no key twice`, () => {
      // GIVEN the catalogue as it is on disk, before any parser drops a key
      const source = readCatalogue(name);

      // WHEN every object is scanned for a repeated key
      const duplicates = findDuplicateKeys(source);

      // THEN none is repeated, so no translation is silently overwritten
      expect(duplicates, describeDuplicateKeys(name, duplicates)).toEqual([]);
    });
  }
});
