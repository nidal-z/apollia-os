import { describe, it, expect } from "vitest";
import {
  tryConnectorPrompt,
  tryQuestions,
  type TryConnectorPromptDeps,
} from "./tryConnectorPrompt";
import type { ChatSessionSummary, Connection, TryExample } from "$lib/types";

const SESSION: ChatSessionSummary = {
  id: "sess-1",
  mode: "libre",
  agent_name: null,
  status: "active",
  last_message_preview: null,
  message_count: 0,
  created_at: "2026-09-30T09:00:00Z",
  closed_at: null,
  title: null,
};

function connection(id: string, tools: string[], overrides: Partial<Connection> = {}): Connection {
  return {
    id,
    kind: "mcp",
    label: id,
    account: null,
    provenance: "selection",
    remote: false,
    official: false,
    state: "connected",
    tools: tools.map((t) => ({ id: t, family: "read", familySource: "selection", prechecked: true })),
    ...overrides,
  };
}

const NOTION = connection("mcp:notion", ["mcp:notion/search", "mcp:notion/fetch"]);
const GOOGLE = connection("native:google:a@b.fr", ["gmail.search.a@b.fr"], { kind: "native" });
const FOLDER = connection("folder:f1", ["file_read", "file_list"], { kind: "folder" });

interface Recorder {
  deps: TryConnectorPromptDeps;
  createdWith: string[][];
  sent: Array<[string, string]>;
  named: Array<[string, string]>;
  remembered: string[];
  reported: unknown[];
  navigations: number;
}

function recorder(overrides: Partial<TryConnectorPromptDeps> = {}): Recorder {
  const r: Recorder = {
    createdWith: [],
    sent: [],
    named: [],
    remembered: [],
    reported: [],
    navigations: 0,
    deps: undefined as unknown as TryConnectorPromptDeps,
  };
  r.deps = {
    loadConnections: async () => ({ status: "ready", connections: [GOOGLE, NOTION, FOLDER] }),
    createSession: async (tools) => {
      r.createdWith.push(tools);
      return SESSION;
    },
    sendMessage: async (id, prompt) => {
      r.sent.push([id, prompt]);
    },
    nameSession: (id, prompt) => {
      r.named.push([id, prompt]);
    },
    rememberSession: (id) => {
      r.remembered.push(id);
    },
    report: (err) => {
      r.reported.push(err);
    },
    navigate: () => {
      r.navigations += 1;
    },
    noToolsMessage: "no tools",
    notFoundMessage: "gone",
    ...overrides,
  };
  return r;
}

describe("tryConnectorPrompt", () => {
  it("opens a chat limited to the connector tools, sends the prompt and routes to it", async () => {
    // GIVEN a server connection that exposes two tools among other connections
    const r = recorder();
    // WHEN the operator tries a prompt on it
    const ok = await tryConnectorPrompt("mcp:notion", "List my databases", r.deps);
    // THEN the session is limited to those tools, the prompt is sent and the route changes
    expect(ok).toBe(true);
    expect(r.createdWith).toEqual([["mcp:notion/search", "mcp:notion/fetch"]]);
    expect(r.sent).toEqual([["sess-1", "List my databases"]]);
    expect(r.named).toEqual([["sess-1", "List my databases"]]);
    expect(r.remembered).toEqual(["sess-1"]);
    expect(r.navigations).toBe(1);
    expect(r.reported).toEqual([]);
  });

  it("reports and stays put when the connector exposes no tool", async () => {
    // GIVEN a connection with no tool
    const empty = connection("mcp:empty", []);
    const r = recorder({ loadConnections: async () => ({ status: "ready", connections: [empty] }) });
    // WHEN the operator tries a prompt
    const ok = await tryConnectorPrompt("mcp:empty", "x", r.deps);
    // THEN no session is created, nothing is sent and the operator is told why
    expect(ok).toBe(false);
    expect(r.createdWith).toEqual([]);
    expect(r.sent).toEqual([]);
    expect(r.navigations).toBe(0);
    expect((r.reported[0] as Error).message).toBe("no tools");
  });

  it("reports and stays put when the session cannot be created", async () => {
    // GIVEN a backend that refuses to create the session (no model configured)
    const failure = new Error("no llm");
    const r = recorder({
      createSession: async () => {
        throw failure;
      },
    });
    // WHEN the operator tries a prompt
    const ok = await tryConnectorPrompt("mcp:notion", "x", r.deps);
    // THEN the failure is reported, no id is handed over and the route is unchanged
    expect(ok).toBe(false);
    expect(r.reported).toEqual([failure]);
    expect(r.remembered).toEqual([]);
    expect(r.navigations).toBe(0);
  });

  it("still routes to the session when only the opening send fails", async () => {
    // GIVEN a session that exists but whose first message cannot be sent
    const r = recorder({
      sendMessage: async () => {
        throw new Error("send failed");
      },
    });
    // WHEN the operator tries a prompt
    const ok = await tryConnectorPrompt("mcp:notion", "x", r.deps);
    // THEN the operator still lands on the session and can retype the message
    expect(ok).toBe(true);
    expect(r.remembered).toEqual(["sess-1"]);
    expect(r.navigations).toBe(1);
  });

  it("limits a native account and a folder to their own tools", async () => {
    // GIVEN a native account and a folder among the connections
    const r = recorder();
    // WHEN each is tried
    await tryConnectorPrompt("native:google:a@b.fr", "x", r.deps);
    await tryConnectorPrompt("folder:f1", "y", r.deps);
    // THEN each session is limited to the tools of its connection
    expect(r.createdWith).toEqual([["gmail.search.a@b.fr"], ["file_read", "file_list"]]);
  });

  it("reports and stays put when the connection is no longer listed", async () => {
    // GIVEN a connection removed since the dialog opened
    const r = recorder();
    // WHEN the operator tries it
    const ok = await tryConnectorPrompt("mcp:removed", "x", r.deps);
    // THEN nothing is created and the operator is told why
    expect(ok).toBe(false);
    expect(r.createdWith).toEqual([]);
    expect((r.reported[0] as Error).message).toBe("gone");
  });

  it("reports the loader failure and stays put", async () => {
    // GIVEN a connection list that cannot be read
    const r = recorder({ loadConnections: async () => ({ status: "error", message: "down" }) });
    // WHEN the operator tries a prompt
    const ok = await tryConnectorPrompt("mcp:notion", "x", r.deps);
    // THEN the loader's words are reported
    expect(ok).toBe(false);
    expect((r.reported[0] as Error).message).toBe("down");
  });
});

describe("tryQuestions", () => {
  const general = { label: "General question", question: "What can I do with Notion?" };
  const examples: TryExample[] = [
    {
      tool: "search",
      label: { en: "search pages", fr: "chercher des pages" },
      question: { en: "Which pages mention the budget?", fr: "Quelles pages parlent du budget ?" },
    },
    {
      tool: "create",
      label: { en: "create a page", fr: "créer une page" },
      question: { en: "Create a page.", fr: "Crée une page." },
    },
  ];

  it("offers one question per example whose tool the connection publishes, in the locale", () => {
    // GIVEN a server publishing `search` and `fetch`, and examples on `search` and `create`
    // WHEN the questions are built in French
    const questions = tryQuestions(NOTION, examples, "fr-FR", general);
    // THEN only the `search` example is offered, in French
    expect(questions).toEqual([
      {
        id: "search",
        label: "chercher des pages",
        question: "Quelles pages parlent du budget ?",
        general: false,
      },
    ]);
  });

  it("falls back to English when the locale has no text", () => {
    // GIVEN examples in English and French
    // WHEN the questions are built in German
    const questions = tryQuestions(NOTION, examples, "de", general);
    // THEN the English text is offered
    expect(questions[0]?.question).toBe("Which pages mention the budget?");
  });

  it("offers the general question to a connection without example", () => {
    // GIVEN a folder, which carries no example
    // WHEN the questions are built
    const questions = tryQuestions(FOLDER, [], "fr", general);
    // THEN the only question is the general one
    expect(questions).toEqual([
      { id: "general", label: "General question", question: "What can I do with Notion?", general: true },
    ]);
  });

  it("offers the general question when no example matches a published tool", () => {
    // GIVEN examples on tools the connection does not publish
    const other = connection("mcp:other", ["mcp:other/list"]);
    // WHEN the questions are built
    const questions = tryQuestions(other, examples, "en", general);
    // THEN the general question is offered
    expect(questions.map((q) => q.id)).toEqual(["general"]);
  });
});
