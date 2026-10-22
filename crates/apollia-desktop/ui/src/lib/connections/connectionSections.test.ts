import { describe, test, expect } from "vitest";
import {
  agentsToOffer,
  canReclassify,
  familyLabel,
  grantedTools,
  toolPrefixOf,
  usedDescriptor,
} from "./connectionSections";
import type { AgentListItem, Connection, ConnectionTool } from "$lib/types";

function tool(id: string, familySource: ConnectionTool["familySource"] = "server"): ConnectionTool {
  return { id, family: "read", familySource, prechecked: false };
}

function installed(name: string, agentType: AgentListItem["agent_type"] = null): AgentListItem {
  return { id: null, name, agent_type: agentType } as AgentListItem;
}

describe("connection card sections - decisions", () => {
  test("each kind is read by the prefix its tools are journaled under", () => {
    // GIVEN one connection of each kind
    // WHEN the prefix of each is read
    // THEN a service, an account and a folder have theirs, a mailbox none
    expect(toolPrefixOf({ id: "mcp:notion", kind: "mcp" })).toBe("mcp:notion/");
    expect(toolPrefixOf({ id: "native:google:ada@example.com", kind: "native" })).toBe(
      "conn:google:ada@example.com/",
    );
    expect(toolPrefixOf({ id: "folder:4f1c", kind: "folder" })).toBe("conn:folder:4f1c/");
    expect(toolPrefixOf({ id: "imap:ada@example.com", kind: "imap" })).toBeNull();
  });

  test("only a tool of a service outside the selection may change family", () => {
    // GIVEN a custom service, a service from the selection, and an account
    const custom = { kind: "mcp", provenance: "custom" } as Pick<Connection, "kind" | "provenance">;
    const curated = { kind: "mcp", provenance: "selection" } as Pick<Connection, "kind" | "provenance">;
    const account = { kind: "native", provenance: "custom" } as Pick<Connection, "kind" | "provenance">;

    // WHEN each tool is asked whether it may change
    // THEN a custom tool may, one the selection classifies, a curated service's and an account's may not
    expect(canReclassify(custom, tool("mcp:acme/create"))).toBe(true);
    expect(canReclassify(custom, tool("mcp:acme/create", "operator"))).toBe(true);
    expect(canReclassify(custom, tool("mcp:acme/create", "selection"))).toBe(false);
    expect(canReclassify(curated, tool("mcp:notion/search"))).toBe(false);
    expect(canReclassify(account, tool("conn:google:ada/gmail.send", "builtin"))).toBe(false);
  });

  test("Give to an agent offers Apollia then the agents that do not use it yet", () => {
    // GIVEN three installed agents, one of which already holds a tool
    const agents = [installed("writer"), installed("analyst"), installed("watch")];
    const holders = [{ id: "watch", name: "watch", enabled: true, byDefault: false, tools: ["mcp:acme/a"] }];

    // WHEN the offer is built
    const offer = agentsToOffer(agents, holders, "Apollia");

    // THEN Apollia comes first, the others by name, the holder left out
    expect(offer.map((agent) => agent.id)).toEqual(["apollia:chat", "analyst", "writer"]);
    expect(offer[0]?.name).toBe("Apollia");
  });

  test("Give to an agent leaves the system agents out, as the add window does", () => {
    // GIVEN a worker, an assistant and a system agent, none holding the connection
    const agents = [installed("writer", "worker"), installed("helper", "assistant"), installed("apollia-guide", "system")];

    // WHEN the offer is built
    const offer = agentsToOffer(agents, [], "Apollia");

    // THEN the system agent is not offered
    expect(offer.map((agent) => agent.id)).toEqual(["apollia:chat", "helper", "writer"]);
  });

  test("a grant from the card gives every tool of the connection", () => {
    // GIVEN a connection with two tools
    const connection = { tools: [tool("mcp:acme/a"), tool("mcp:acme/b")] };
    // WHEN the tools of the grant are read
    // THEN both are given
    expect(grantedTools(connection)).toEqual(["mcp:acme/a", "mcp:acme/b"]);
  });

  test("a family is named in plain words, an unknown one as unclassified", () => {
    // GIVEN a family the screens name and one they do not
    // WHEN their labels are read
    // THEN the first has its own, the second reads unclassified
    expect(familyLabel("send_external")).toBe("agent_sheet.family_send_external");
    expect(familyLabel("unclassified")).toBe("agent_sheet.family_unclassified");
    expect(familyLabel("something_new")).toBe("agent_sheet.family_unclassified");
  });

  test("the last use is said in minutes, hours, then days", () => {
    // GIVEN a reference instant
    const now = Date.parse("2026-10-22T12:00:00Z");
    // WHEN uses at several distances are described
    // THEN each reads at its scale, and nothing is said without a date
    expect(usedDescriptor("2026-10-22T11:58:00Z", now)).toEqual({
      key: "connections.card.activity.used_minutes_ago",
      values: { mins: 2 },
    });
    expect(usedDescriptor("2026-10-22T09:00:00Z", now)?.values).toEqual({ hours: 3 });
    expect(usedDescriptor("2026-10-19T12:00:00Z", now)?.values).toEqual({ days: 3 });
    expect(usedDescriptor(null, now)).toBeNull();
  });
});
