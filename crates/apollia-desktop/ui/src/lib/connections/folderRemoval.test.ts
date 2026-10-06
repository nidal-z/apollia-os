import { describe, expect, it } from "vitest";
import type { Source } from "$lib/ipc/sources";
import { dependentSources } from "./folderRemoval";

function source(over: Partial<Source>): Source {
  return {
    id: "src-1",
    kind: "folder",
    label: "Invoices",
    group: null,
    reference: "folder:4f1c",
    indexState: "ready",
    addedAt: "2026-10-01T08:00:00.000Z",
    ...over,
  };
}

describe("dependentSources", () => {
  it("lists the folder and code folder sources made on the connection, whatever the spelling of the reference", () => {
    // GIVEN two sources on the folder, one named with the prefix and one without,
    // a source on another folder, and a file whose path looks like the identifier
    const sources = [
      source({ id: "src-a", label: "Supplier invoices" }),
      source({ id: "src-b", label: "Scripts", kind: "code_folder", reference: "4f1c", indexState: "indexing" }),
      source({ id: "src-c", reference: "folder:9e2d" }),
      source({ id: "src-d", kind: "file", reference: "4f1c" }),
    ];

    // WHEN the removal of the folder connection is prepared
    const dependents = dependentSources(sources, "folder:4f1c");

    // THEN only the two sources made on it are listed, with their state
    expect(dependents).toEqual([
      { id: "src-a", label: "Supplier invoices", kind: "folder", indexState: "ready" },
      { id: "src-b", label: "Scripts", kind: "code_folder", indexState: "indexing" },
    ]);
  });

  it("lists nothing for a folder no source was made on", () => {
    // GIVEN a registry whose only source is on another folder
    // WHEN the removal of a folder nothing depends on is prepared
    // THEN there is nothing to confirm
    expect(dependentSources([source({ reference: "folder:9e2d" })], "folder:4f1c")).toEqual([]);
  });
});
