import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  REFUSAL_TEXT_KEYS,
  REMOTE_DENIED_REASON,
  addConnection,
  nodePercent,
  refusalOf,
  testErrorKey,
  tryAddedConnection,
  type NodeRuntimeState,
} from "$lib/integrations/addConnection";
import type { WizardInvoke } from "$lib/ipc/connections";
import type { McpServerConfigInput } from "$lib/types";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { readFileSync } from "node:fs";

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

/** Fake commands: each answer is a value, or a function of the call that may throw. */
function fakeInvoke(
  calls: Call[],
  answers: Record<string, unknown | ((args: Record<string, unknown>) => unknown)>,
): WizardInvoke {
  return (async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args: args ?? {} });
    const answer = answers[cmd];
    return typeof answer === "function"
      ? (answer as (a: Record<string, unknown>) => unknown)(args ?? {})
      : answer;
  }) as WizardInvoke;
}

function config(over: Partial<McpServerConfigInput> = {}): McpServerConfigInput {
  return {
    name: "notion",
    url: "https://mcp.notion.com/mcp",
    env: {},
    transport: "streamable-http",
    tags: [],
    ...over,
  };
}

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node as Record<string, unknown> | undefined)?.[part],
    catalogue,
  );
}

describe("adding a connection after its warning", () => {
  test("the secrets are filed, then the server is added with the warning accepted", async () => {
    // GIVEN a connection with one secret typed in the authentication step
    const calls: Call[] = [];
    const invokeFn = fakeInvoke(calls, {});

    // WHEN it is added
    const outcome = await addConnection(config(), [{ envVar: "NOTION_TOKEN", value: "t" }], {
      invokeFn,
    });

    // THEN the secret goes first, then the addition, which says the warning was accepted
    expect(outcome).toEqual({ kind: "added" });
    expect(calls.map((c) => c.cmd)).toEqual(["store_mcp_secret", "add_mcp_server"]);
    expect(calls[1].args).toEqual({ config: config(), warningAccepted: true });
  });

  test("a server that runs on Node.js shows the download of Node.js while it waits", async () => {
    // GIVEN an npx server, and Node.js downloading then ready while it is added
    const calls: Call[] = [];
    const states: NodeRuntimeState[] = [
      { state: "downloading", received: 10, total: 40 },
      { state: "downloading", received: 40, total: 40 },
      { state: "ready", tag: "v22.11.0", path: "/engines/node" },
    ];
    let read = 0;
    let release: () => void = () => {};
    const added = new Promise<void>((resolve) => (release = resolve));
    const invokeFn = fakeInvoke(calls, {
      node_runtime_state: () => states[Math.min(read++, states.length - 1)],
      add_mcp_server: () => added,
    });
    const seen: NodeRuntimeState[] = [];

    // WHEN it is added, the addition finishing once Node.js is ready
    const pending = addConnection(
      config({ name: "linear", url: undefined, command: "npx", args: ["-y", "linear"], transport: "stdio" }),
      [],
      {
        invokeFn,
        onNode: (s) => seen.push(s),
        readNode: () => invokeFn<NodeRuntimeState>("node_runtime_state"),
        pollMs: 5,
      },
    );
    await vi.waitFor(() => expect(seen.some((s) => s.state === "ready")).toBe(true));
    release();
    const outcome = await pending;

    // THEN the progress was read before the installation ended, and the reads stop with it
    expect(outcome).toEqual({ kind: "added" });
    expect(seen[0]).toEqual({ state: "downloading", received: 10, total: 40 });
    expect(nodePercent(seen[0])).toBe(25);
    const readsAtEnd = calls.filter((c) => c.cmd === "node_runtime_state").length;
    await new Promise((r) => setTimeout(r, 30));
    expect(calls.filter((c) => c.cmd === "node_runtime_state").length).toBe(readsAtEnd);
  });

  test("a remote server never asks for Node.js", async () => {
    // GIVEN a remote server
    const calls: Call[] = [];
    const invokeFn = fakeInvoke(calls, {});

    // WHEN it is added with a progress listener
    await addConnection(config(), [], {
      invokeFn,
      onNode: () => {},
      readNode: () => invokeFn<NodeRuntimeState>("node_runtime_state"),
      pollMs: 5,
    });

    // THEN Node.js is never read
    expect(calls.map((c) => c.cmd)).toEqual(["add_mcp_server"]);
  });

  test("an admission refusal says who refused and why", async () => {
    // GIVEN an organisation policy that denies online connections
    const calls: Call[] = [];
    const invokeFn = fakeInvoke(calls, {
      add_mcp_server: () => {
        throw REMOTE_DENIED_REASON;
      },
    });

    // WHEN a remote server is added
    const outcome = await addConnection(config(), [], { invokeFn });

    // THEN the outcome is a refusal by the organisation, with its code
    expect(outcome).toEqual({
      kind: "refused",
      refusal: { code: "remote_denied", by: "organisation", reason: REMOTE_DENIED_REASON },
    });
  });

  test("a failure that is not a refusal is reported as is", async () => {
    // GIVEN a server whose process does not start
    const invokeFn = fakeInvoke([], {
      add_mcp_server: () => {
        throw new Error("spawn ENOENT");
      },
    });

    // WHEN it is added
    const outcome = await addConnection(config(), [], { invokeFn });

    // THEN the failure carries its message
    expect(outcome).toEqual({ kind: "failed", message: "spawn ENOENT" });
  });

  test("an addition without any Node.js is told apart by its code", async () => {
    // GIVEN a desktop that finds no Node.js for an `npx` server
    const message = "node_unavailable: This service needs Node.js.";
    const invokeFn = fakeInvoke([], {
      add_mcp_server: () => {
        throw message;
      },
    });

    // WHEN it is added
    const outcome = await addConnection(config({ command: "npx" }), [], { invokeFn });

    // THEN the failure is the absence of Node.js, its detail kept
    expect(outcome).toEqual({ kind: "failed", message, reason: "node_unavailable" });
  });
});

describe("the refusals of the admission", () => {
  test("the reason the runtime gives is the one the dialog recognises", () => {
    // GIVEN the runtime source of the reason of a denied online connection
    const source = readFileSync(
      new URL("../../../../../apollia-runtime/src/governance/policy_admission.rs", import.meta.url),
      "utf8",
    );

    // WHEN its constant is read
    const declared = /REMOTE_DENIED_REASON: &str =\s*"([^"]+)"/.exec(source)?.[1];

    // THEN it is the reason the dialog maps to its code
    expect(declared).toBe(REMOTE_DENIED_REASON);
    expect(refusalOf(declared ?? "")?.code).toBe("remote_denied");
  });

  test("an unreadable policy is a refusal by the organisation", () => {
    // GIVEN the reason the admission gives when the policy cannot be read
    const reason = "the connection policy could not be read: the organisation policy is refused";

    // WHEN it is classified
    const refusal = refusalOf(reason);

    // THEN it is a refusal of its own code
    expect(refusal).toEqual({ code: "policy_unreadable", by: "organisation", reason });
    expect(refusalOf("spawn ENOENT")).toBeNull();
  });

  test("a connection the policy denies or leaves out is a refusal by the organisation", () => {
    // GIVEN the runtime source of the reasons of the two list rules
    const source = readFileSync(
      new URL("../../../../../apollia-runtime/src/governance/policy_admission.rs", import.meta.url),
      "utf8",
    );
    const denied = /CONNECTION_DENIED_REASON: &str =\s*"([^"]+)"/.exec(source)?.[1] ?? "";
    const notAllowed = /CONNECTION_NOT_ALLOWED_REASON: &str =\s*"([^"]+)"/.exec(source)?.[1] ?? "";

    // WHEN each is classified
    const refusals = [denied, notAllowed].map(refusalOf);

    // THEN each is a refusal by the organisation, told in both languages
    expect(refusals).toEqual([
      { code: "policy_denied", by: "organisation", reason: denied },
      { code: "policy_denied", by: "organisation", reason: notAllowed },
    ]);
    const texts = [lookup(en, "connections.refusal.policy_denied"), lookup(fr, "connections.refusal.policy_denied")];
    expect(texts.every((text) => typeof text === "string" && text.length > 0)).toBe(true);
    expect(texts[0]).not.toBe(texts[1]);
  });

  test("every refusal code has its texts in both languages", () => {
    // GIVEN the keys of each refusal code
    const keys = Object.values(REFUSAL_TEXT_KEYS).flatMap((k) => [k.title, k.body]);

    // WHEN they are looked up in each catalogue
    const missing = keys.filter(
      (k) => typeof lookup(en, k) !== "string" || typeof lookup(fr, k) !== "string",
    );

    // THEN none is missing
    expect(missing).toEqual([]);
  });
});

describe("the trial once added", () => {
  test("a working connection reports its tools", async () => {
    // GIVEN a live test that answers two tools and a healthy session
    const invokeFn = fakeInvoke([], {
      test_mcp_live_server: {
        kind: "success",
        server_info: "notion",
        protocol_version: "2025-06-18",
        tools: [{ name: "a" }, { name: "b" }],
        test_duration_ms: 12,
        live_health: { state: "healthy", verified: true },
      },
    });

    // WHEN the added connection is tried
    const outcome = await tryAddedConnection("notion", invokeFn);

    // THEN it succeeded with two tools
    expect(outcome).toEqual({ kind: "succeeded", tools: 2 });
  });

  test("a trial says which Node.js ran the server", async () => {
    // GIVEN a live test of a server that ran on the Node.js of the machine
    const invokeFn = fakeInvoke([], {
      test_mcp_live_server: {
        kind: "success",
        server_info: "browser",
        protocol_version: "2025-06-18",
        tools: [{ name: "open" }],
        test_duration_ms: 3,
        live_health: null,
        node_source: "machine",
      },
    });

    // WHEN the added connection is tried
    const outcome = await tryAddedConnection("browser", invokeFn);

    // THEN the outcome carries where its Node.js came from
    expect(outcome).toEqual({ kind: "succeeded", tools: 1, nodeSource: "machine" });
  });

  test("a failed trial reads in plain words and keeps the raw detail", async () => {
    // GIVEN a live test that fails on the network
    const invokeFn = fakeInvoke([], {
      test_mcp_live_server: () => {
        throw new Error("transport closed: connection reset");
      },
    });

    // WHEN the added connection is tried
    const outcome = await tryAddedConnection("notion", invokeFn);

    // THEN the message is the plain one, the detail the raw one
    expect(outcome).toEqual({
      kind: "failed",
      messageKey: "integrations.wizard.test_err.transport_closed",
      detail: "transport closed: connection reset",
    });
  });

  test("a reachable server whose calls fail is a failed trial", async () => {
    // GIVEN a live test whose session is unavailable
    const invokeFn = fakeInvoke([], {
      test_mcp_live_server: {
        kind: "success",
        server_info: "notion",
        protocol_version: "2025-06-18",
        tools: [],
        test_duration_ms: 12,
        live_health: { state: "unavailable", reason: "HTTP 401 Unauthorized" },
      },
    });

    // WHEN the added connection is tried
    const outcome = await tryAddedConnection("notion", invokeFn);

    // THEN the trial failed, read as an authorisation problem
    expect(outcome).toEqual({
      kind: "failed",
      messageKey: "integrations.wizard.test_err.unauthorized",
      detail: "HTTP 401 Unauthorized",
    });
  });

  test("each raw error maps to a message both catalogues hold", () => {
    // GIVEN raw errors of each kind
    const raws = [
      "HTTP 401 Unauthorized",
      "403 Forbidden",
      "404 Not Found",
      "initialize timeout",
      "server exited",
      "spawn ENOENT",
      "command not found: npx",
      "dns lookup failed",
      "gremlin exploded",
    ];

    // WHEN each is mapped
    const keys = raws.map(testErrorKey);

    // THEN each is a distinct plain message, present in both languages
    expect(keys).toEqual([
      "integrations.wizard.test_err.unauthorized",
      "integrations.wizard.test_err.forbidden",
      "integrations.wizard.test_err.not_found",
      "integrations.wizard.test_err.handshake_timeout",
      "integrations.wizard.test_err.transport_closed",
      "integrations.wizard.test_err.command_missing",
      "integrations.wizard.test_err.command_missing",
      "integrations.wizard.test_err.network",
      "integrations.wizard.test_err.generic",
    ]);
    for (const key of keys) {
      expect(typeof lookup(en, key)).toBe("string");
      expect(typeof lookup(fr, key)).toBe("string");
    }
  });
});
