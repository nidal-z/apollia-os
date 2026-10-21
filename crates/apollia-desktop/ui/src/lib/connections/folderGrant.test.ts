import { beforeEach, describe, expect, test, vi } from "vitest";
import type { Connection } from "$lib/types";

const getAgentDefinition = vi.fn();
const setAgentOverride = vi.fn();
const grantConnection = vi.fn();

vi.mock("$lib/ipc/agentSheet", () => ({ getAgentDefinition }));
vi.mock("$lib/ipc/agentAdjust", () => ({ setAgentOverride }));
vi.mock("$lib/ipc/connections", () => ({ grantConnection }));

const { accessWithFolder, grantFolderToApollia, readingTools } = await import("./folderGrant");

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
  beforeEach(() => {
    getAgentDefinition.mockReset();
    setAgentOverride.mockReset();
    grantConnection.mockReset();
  });

  test("an Apollia on the default access is given the folder on top of it", async () => {
    // GIVEN an Apollia nobody chose an access for, whose card lists the reading family
    getAgentDefinition.mockResolvedValue({ access: ["web_search"], overridden: [] });

    // WHEN the folder is granted to it
    const changed = await grantFolderToApollia(folder());

    // THEN only the reading tools of the folder go to the runtime, and no access list is written
    expect(changed).toBe(true);
    expect(grantConnection).toHaveBeenCalledWith("conn:folder:abc", ["apollia:chat"], [
      "conn:folder:abc/file_read",
      "conn:folder:abc/file_list",
    ]);
    expect(setAgentOverride).not.toHaveBeenCalled();
  });

  test("a folder Apollia already reads writes nothing", async () => {
    // GIVEN an Apollia that holds both reading tools of the folder
    getAgentDefinition.mockResolvedValue({
      access: ["conn:folder:abc/file_read", "conn:folder:abc/file_list"],
      overridden: ["access"],
    });

    // WHEN the folder is granted to it
    const changed = await grantFolderToApollia(folder());

    // THEN nothing goes to the runtime
    expect(changed).toBe(false);
    expect(grantConnection).not.toHaveBeenCalled();
  });

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
