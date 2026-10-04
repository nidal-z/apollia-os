import { describe, test, expect } from "vitest";

import type { Connection, ConnectionTool } from "$lib/types";
import {
  connectionFamily,
  connectionOf,
  connectionToolFamily,
  connectionToolGroups,
  connectionToolName,
  initialPicks,
  remoteAccesses,
} from "./connectionGrant";

function tool(id: string, family: ConnectionTool["family"], prechecked: boolean): ConnectionTool {
  return { id, family, familySource: "selection", prechecked };
}

function connection(over: Partial<Connection> = {}): Connection {
  return {
    id: "mcp:outlook",
    kind: "mcp",
    label: "Outlook",
    account: "support@example.com",
    provenance: "selection",
    remote: true,
    official: true,
    state: "connected",
    tools: [
      tool("mcp:outlook/read_mail", "read", true),
      tool("mcp:outlook/search_mail", "read", true),
      tool("mcp:outlook/read_contacts", "read_personal", false),
      tool("mcp:outlook/move_mail", "write_or_delete", false),
      tool("mcp:outlook/send_mail", "send_external", false),
      tool("mcp:outlook/export_box", "unclassified", false),
    ],
    ...over,
  };
}

describe("connectionToolGroups", () => {
  test("lists the tools in the five families, then unclassified", () => {
    // GIVEN a connection with a tool in four families and one unclassified
    const c = connection();

    // WHEN its tools are grouped
    const groups = connectionToolGroups(c, new Map(), []);

    // THEN the five families come in order, the empty one kept, then unclassified
    expect(groups.map((g) => g.key)).toEqual([
      "read",
      "read_personal",
      "write_or_delete",
      "send_external",
      "execute_code",
      "unclassified",
    ]);
    expect(groups[0].tools.map((t) => t.id)).toEqual(["mcp:outlook/read_mail", "mcp:outlook/search_mail"]);
    expect(groups[4].tools).toEqual([]);
    expect(groups[5].tools.map((t) => t.id)).toEqual(["mcp:outlook/export_box"]);
  });

  test("leaves unclassified out when every tool has a family", () => {
    // GIVEN a connection whose tools are all classified
    const c = connection({ tools: [tool("mcp:notes/read", "read", true)] });

    // WHEN its tools are grouped
    const groups = connectionToolGroups(c, new Map(), []);

    // THEN only the five families are listed
    expect(groups.map((g) => g.key)).not.toContain("unclassified");
    expect(groups).toHaveLength(5);
  });

  test("names each tool, describes it and says which are granted already", () => {
    // GIVEN a description from the tool registry and a tool the draft grants
    const descriptions = new Map([["mcp:outlook/read_mail", "Lists and opens the mails of a folder"]]);

    // WHEN the tools are grouped
    const read = connectionToolGroups(connection(), descriptions, ["mcp:outlook/search_mail"])[0];

    // THEN the rows carry a readable name, the description and the granted mark
    expect(read.tools[0]).toMatchObject({
      name: "Read mail",
      description: "Lists and opens the mails of a folder",
      granted: false,
      prechecked: true,
    });
    expect(read.tools[1]).toMatchObject({ name: "Search mail", description: "", granted: true });
  });
});

describe("initialPicks", () => {
  test("checks only the reading tools of a connection from the selection", () => {
    // GIVEN a connection of the selection, as the runtime offers it
    const c = connection();

    // WHEN the dialog opens on it
    const picks = initialPicks(c, []);

    // THEN its two reading tools are checked and nothing else
    expect(picks).toEqual(["mcp:outlook/read_mail", "mcp:outlook/search_mail"]);
  });

  test("checks nothing on a connection whose tools the runtime does not precheck", () => {
    // GIVEN a custom connection: the runtime prechecks none of its tools, reading included
    const c = connection({
      provenance: "custom",
      official: false,
      tools: [tool("mcp:mine/read", "read", false), tool("mcp:mine/write", "write_or_delete", false)],
    });

    // WHEN the dialog opens on it
    const picks = initialPicks(c, []);

    // THEN every box starts unchecked
    expect(picks).toEqual([]);
  });

  test("leaves out a prechecked tool the draft already grants", () => {
    // GIVEN a draft that already grants one reading tool
    const granted = ["mcp:outlook/read_mail"];

    // WHEN the dialog opens on the connection
    const picks = initialPicks(connection(), granted);

    // THEN only the other reading tool is added
    expect(picks).toEqual(["mcp:outlook/search_mail"]);
  });
});

describe("remote connections on the sheet", () => {
  const local = connection({
    id: "mcp:files",
    label: "Files",
    account: null,
    remote: false,
    tools: [tool("mcp:files/read", "read", true)],
  });

  test("marks every access that comes from a remote connection, and only those", () => {
    // GIVEN a remote and a local connection, and accesses of both, a native tool and an agent
    const accesses = ["mcp:outlook/send_mail", "mcp:outlook", "mcp:files/read", "file_read", "agent:veille"];

    // WHEN the remote accesses are computed
    const remote = remoteAccesses(accesses, [connection(), local]);

    // THEN the tool and the whole server of the remote connection are marked
    expect([...remote].sort()).toEqual(["mcp:outlook", "mcp:outlook/send_mail"]);
  });

  test("finds the connection and the family of a tool of a connection", () => {
    // GIVEN the connections of the machine
    const all = [connection(), local];

    // WHEN a tool of a connection is looked up, then a native tool
    // THEN the first is found with its family, the second is not
    expect(connectionOf("mcp:files/read", all)?.id).toBe("mcp:files");
    expect(connectionToolFamily("mcp:outlook/send_mail", all)).toBe("send_external");
    expect(connectionOf("file_read", all)).toBeNull();
    expect(connectionToolFamily("file_read", all)).toBeNull();
  });
});

describe("connectionFamily and connectionToolName", () => {
  test("files an unknown family as unclassified and names a tool by its last segment", () => {
    // GIVEN a family this build does not know, and a native tool of an account
    // WHEN they are read
    // THEN the family is unclassified, exempt reads with read, and the name is readable
    expect(connectionFamily("teleport")).toBe("unclassified");
    expect(connectionFamily("exempt")).toBe("read");
    expect(connectionToolName("conn:google:ada@example.com/gmail.send")).toBe("Gmail send");
  });
});
