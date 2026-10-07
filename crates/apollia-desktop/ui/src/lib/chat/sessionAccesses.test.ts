import { describe, expect, test } from "vitest";
import type { SessionSettings } from "$lib/chat/sessionSettings";
import {
  accessKind,
  agentProfileSwitch,
  groupSessionAccesses,
  isSwitchedOn,
  profileSwitchCase,
  PROFILE_SWITCH_KEYS,
  sessionAccessRows,
  switchSessionAccess,
  type SessionAccessInput,
} from "./sessionAccesses";

const INPUT: SessionAccessInput = {
  held: ["web_search", "file_read", "mcp:gmail/draft_message", "workflow:sales/follow_up"],
  tools: [
    { name: "web_search", description: "Search the web", kind: "native" },
    { name: "file_read", description: "Read a file", kind: "native" },
    { name: "bash_executor", description: "Run a command", kind: "native" },
    { name: "mcp:gmail/draft_message", description: "Prepare a message", kind: "mcp" },
    { name: "mcp:notion/search_pages", description: "Search the pages", kind: "mcp" },
  ],
  targets: [
    { kind: "agent", item_id: "accountant", name: "Accountant", description: "Keeps the books" },
    { kind: "agent", item_id: "market-watch", name: "Market watch", description: "Reads the tenders" },
    { kind: "workflow", item_id: "sales/follow_up", name: "Quote follow-up", description: "" },
  ],
  self: "accountant",
  native: new Map([["web_search", { nameKey: "tools.catalog.web_search.label_operator", detailKey: "tools.catalog.web_search.desc_operator" }]]),
};

function row(id: string, settings: SessionSettings = {}) {
  const found = sessionAccessRows(INPUT, settings).find((r) => r.id === id);
  if (!found) throw new Error(`no line for ${id}`);
  return found;
}

describe("the accesses of the agent of a conversation are listed by group", () => {
  test("tools, connections, workflows and other agents each have their group", () => {
    // GIVEN an agent that holds two tools, a tool of a connection and a workflow
    // WHEN the tab lists what it reaches
    const groups = groupSessionAccesses(sessionAccessRows(INPUT, {}), "");

    // THEN the four groups come in order, the accesses of the agent first in each
    expect(groups.map((g) => g.kind)).toEqual(["tools", "connections", "workflows", "agents"]);
    expect(groups[0].rows.map((r) => r.id)).toEqual(["web_search", "file_read", "bash_executor"]);
    expect(groups[1].rows.map((r) => r.id)).toEqual(["mcp:gmail/draft_message", "mcp:notion/search_pages"]);
    expect(groups[2].rows.map((r) => r.name)).toEqual(["Quote follow-up"]);
    expect(groups[3].rows.map((r) => r.name)).toEqual(["Market watch"]);
  });

  test("the agent is never offered to itself", () => {
    // GIVEN the agent "accountant" among the agents that can be called
    // WHEN its own conversation lists the other agents
    const ids = sessionAccessRows(INPUT, {}).map((r) => r.id);

    // THEN it is not one of them
    expect(ids).not.toContain("agent:accountant");
  });

  test("a line reads in plain words, the raw identifier kept aside", () => {
    // GIVEN a native tool with catalogue words, a tool of a connection, another agent
    // WHEN their lines are read
    // THEN each carries a name a person reads, and its identifier apart
    expect(row("web_search").nameKey).toBe("tools.catalog.web_search.label_operator");
    expect(row("web_search").detailKey).toBe("tools.catalog.web_search.desc_operator");
    expect(row("mcp:notion/search_pages").name).toBe("Notion · Search pages");
    expect(row("mcp:notion/search_pages").detail).toBe("Search the pages");
    expect(row("agent:market-watch").name).toBe("Market watch");
    expect(row("file_read").name).toBe("File read");
  });

  test("the group of an access follows what it is, not where it was declared", () => {
    // GIVEN identifiers of each sort
    // WHEN each is classified
    // THEN a tool of a connection is a connection, whatever its prefix
    expect(accessKind("agent:a")).toBe("agents");
    expect(accessKind("workflow:p/w")).toBe("workflows");
    expect(accessKind("mcp:notion")).toBe("connections");
    expect(accessKind("conn:work/gmail_send")).toBe("connections");
    expect(accessKind("search_pages", { name: "search_pages", description: "", kind: "mcp" })).toBe("connections");
    expect(accessKind("file_read", { name: "file_read", description: "", kind: "native" })).toBe("tools");
    expect(accessKind("unknown_tool")).toBe("tools");
  });
});

describe("the filter keeps the lines whose name holds what is typed", () => {
  test("a name filters across groups and drops the groups left empty", () => {
    // GIVEN the list of the agent
    const rows = sessionAccessRows(INPUT, {});

    // WHEN "notion" is typed in the filter
    const groups = groupSessionAccesses(rows, "  Notion ");

    // THEN the one matching line is left, in its group alone
    expect(groups.map((g) => g.kind)).toEqual(["connections"]);
    expect(groups[0].rows.map((r) => r.id)).toEqual(["mcp:notion/search_pages"]);
  });

  test("a native tool is found by the name the interface shows", () => {
    // GIVEN a translation of the catalogue name of the web search
    const rows = sessionAccessRows(INPUT, {});
    const shown = (r: { id: string; name: string }) => (r.id === "web_search" ? "Recherche sur le web" : r.name);

    // WHEN "recherche" is typed
    const groups = groupSessionAccesses(rows, "recherche", shown);

    // THEN the web search is the line kept
    expect(groups.flatMap((g) => g.rows.map((r) => r.id))).toEqual(["web_search"]);
  });

  test("a raw identifier is searched in developer mode only", () => {
    // GIVEN the list of the agent
    const rows = sessionAccessRows(INPUT, {});

    // WHEN "bash_executor" is typed with the identifiers hidden, then shown
    const hidden = groupSessionAccesses(rows, "bash_executor");
    const shown = groupSessionAccesses(rows, "bash_executor", (r) => r.name, true);

    // THEN only the developer view finds the line by its identifier
    expect(hidden).toEqual([]);
    expect(shown.flatMap((g) => g.rows.map((r) => r.id))).toEqual(["bash_executor"]);
  });
});

describe("a switch cuts an access of the agent for the conversation alone", () => {
  test("switching off an access of the agent stores it as switched off here", () => {
    // GIVEN a conversation with a temperature of its own
    const settings: SessionSettings = { temperature: 0.4 };

    // WHEN the web search of the agent is switched off
    const next = switchSessionAccess(settings, row("web_search"), false);

    // THEN the conversation names it, its other settings untouched
    expect(next).toEqual({ temperature: 0.4, disabled_accesses: ["web_search"] });
    expect(row("web_search", next).state).toBe("off");
    expect(isSwitchedOn(row("web_search", next))).toBe(false);
    // AND nothing is proposed: the agent already holds it
    expect(next.proposed_accesses).toBeUndefined();
  });

  test("switching it back on leaves no trace in the conversation", () => {
    // GIVEN a conversation that switched the web search off
    const settings: SessionSettings = { disabled_accesses: ["web_search"] };

    // WHEN it is switched back on
    const next = switchSessionAccess(settings, row("web_search", settings), true);

    // THEN the list is dropped rather than stored empty
    expect(next).toEqual({});
    expect(row("web_search", next).state).toBe("on");
  });

  test("switching the same access off twice names it once", () => {
    // GIVEN a conversation that already switched the web search off
    const settings: SessionSettings = { disabled_accesses: ["web_search"] };

    // WHEN the switch is written off again
    const next = switchSessionAccess(settings, row("web_search", settings), false);

    // THEN the conversation still names it once
    expect(next.disabled_accesses).toEqual(["web_search"]);
  });
});

describe("an access the agent does not hold is listed and can be proposed", () => {
  test("it is listed as not granted, its switch off", () => {
    // GIVEN a tool and an agent the agent of the conversation does not hold
    // WHEN their lines are read
    // THEN both are listed, not granted, off
    for (const id of ["bash_executor", "agent:market-watch", "mcp:notion/search_pages"]) {
      expect(row(id).held, id).toBe(false);
      expect(row(id).state, id).toBe("not_granted");
      expect(isSwitchedOn(row(id)), id).toBe(false);
    }
  });

  test("switching it on proposes it for the conversation, and grants nothing", () => {
    // GIVEN a conversation whose agent does not hold the command tool
    // WHEN its switch is turned on
    const next = switchSessionAccess({}, row("bash_executor"), true);

    // THEN the conversation proposes it, which makes each of its calls ask
    expect(next).toEqual({ proposed_accesses: ["bash_executor"] });
    expect(row("bash_executor", next).state).toBe("proposed");
    expect(isSwitchedOn(row("bash_executor", next))).toBe(true);
    // AND it is still not an access of the agent
    expect(row("bash_executor", next).held).toBe(false);
  });

  test("switching it off again withdraws the proposal", () => {
    // GIVEN a conversation that proposes the command tool and another agent
    const settings: SessionSettings = { proposed_accesses: ["bash_executor", "agent:market-watch"] };

    // WHEN the command tool is switched off
    const next = switchSessionAccess(settings, row("bash_executor", settings), false);

    // THEN only the other proposal is left
    expect(next).toEqual({ proposed_accesses: ["agent:market-watch"] });
    expect(row("bash_executor", next).state).toBe("not_granted");
  });

  test("a proposal also named as switched off reads as off, and proposing clears it", () => {
    // GIVEN settings that both propose and switch off the same access
    const settings: SessionSettings = {
      proposed_accesses: ["bash_executor"],
      disabled_accesses: ["bash_executor"],
    };

    // WHEN the line is read, then switched on
    const before = row("bash_executor", settings).state;
    const next = switchSessionAccess(settings, row("bash_executor", settings), true);

    // THEN switching off had won, and switching on leaves the proposal alone
    expect(before).toBe("not_granted");
    expect(next).toEqual({ proposed_accesses: ["bash_executor"] });
  });
});

describe("the profile switch follows the agent of the conversation", () => {
  test("the built-in assistant and an installed agent do not carry the same switch", () => {
    // GIVEN a free conversation, a conversation with an agent, and none
    // WHEN the case of each is read
    // THEN only the installed agent gets the reading switch
    expect(profileSwitchCase({ mode: "libre", agent_name: null })).toBe("assistant");
    expect(profileSwitchCase({ mode: "agent", agent_name: "accountant" })).toBe("agent");
    expect(profileSwitchCase(null)).toBe("assistant");
    // AND each case has its own label and its own sentence
    expect(PROFILE_SWITCH_KEYS.assistant.label).not.toBe(PROFILE_SWITCH_KEYS.agent.label);
    expect(PROFILE_SWITCH_KEYS.assistant.help).not.toBe(PROFILE_SWITCH_KEYS.agent.help);
  });

  test("an installed agent follows its profile source until the conversation chooses", () => {
    // GIVEN an agent whose profile source is checked, and one whose is not
    // WHEN the conversation made no choice, then one each way
    // THEN the source decides first, the conversation after
    expect(agentProfileSwitch({}, ["profile", "memory"])).toBe(true);
    expect(agentProfileSwitch({}, ["memory"])).toBe(false);
    expect(agentProfileSwitch({ profile_access: false }, ["profile"])).toBe(false);
    expect(agentProfileSwitch({ profile_access: true }, [])).toBe(true);
  });
});
