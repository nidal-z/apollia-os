import { describe, expect, test } from "vitest";
import { accessWithFolder, readingTools } from "./folderGrant";
import type { Connection } from "$lib/types";

function folder(): Connection {
  return {
    id: "conn:folder:abc",
    kind: "folder",
    label: "Downloads",
    account: null,
    path: "/Users/ada/Downloads",
    provenance: "custom",
    remote: false,
    official: false,
    state: "connected",
    tools: [
      { id: "conn:folder:abc/file_read", family: "read", prechecked: true },
      { id: "conn:folder:abc/file_list", family: "read", prechecked: true },
      { id: "conn:folder:abc/file_write", family: "write_or_delete", prechecked: false },
      { id: "conn:folder:abc/file_delete", family: "write_or_delete", prechecked: true },
    ],
  } as unknown as Connection;
}

describe("granting a folder just added to the agent of the chat", () => {
  test("only the reading tools the runtime offers checked are given", () => {
    // GIVEN a folder whose tools read, write and delete
    // WHEN its reading tools are listed
    const tools = readingTools(folder());

    // THEN writing and deleting stay out, whatever the runtime prechecks
    expect(tools).toEqual(["conn:folder:abc/file_read", "conn:folder:abc/file_list"]);
  });

  test("the accesses of the agent gain the folder once, after what they held", () => {
    // GIVEN an agent that reads the web and already holds one tool of the folder
    const held = ["web_search", "conn:folder:abc/file_read"];

    // WHEN the folder is granted, twice
    const once = accessWithFolder(held, folder());
    const twice = accessWithFolder(once, folder());

    // THEN the missing reading tool is added at the end, and nothing is added again
    expect(once).toEqual(["web_search", "conn:folder:abc/file_read", "conn:folder:abc/file_list"]);
    expect(twice).toEqual(once);
  });
});
