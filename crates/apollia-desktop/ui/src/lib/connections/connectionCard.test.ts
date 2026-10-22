import { describe, expect, test, vi } from "vitest";
import {
  checkAndReplace,
  pauseGesture,
  removalLines,
  repairAction,
  replaceConnection,
} from "$lib/connections/connectionCard";
import type { Connection } from "$lib/types";

function connection(over: Partial<Connection> = {}): Connection {
  return {
    id: "mcp:linear",
    kind: "mcp",
    label: "linear",
    account: null,
    provenance: "registry",
    remote: true,
    official: true,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [],
    ...over,
  };
}

describe("repairAction", () => {
  test("each reason a kind can give is repaired by its own action", () => {
    // GIVEN a connection of each kind in need of repair, for its own reason
    const native = connection({ kind: "native", status: "needs_repair", reason: { key: "sign_in_again" } });
    const approval = connection({ kind: "native", status: "needs_repair", reason: { key: "admin_approval_required" } });
    const mailbox = connection({ kind: "imap", status: "needs_repair", reason: { key: "password_refused" } });
    const folder = connection({ kind: "folder", status: "needs_repair", reason: { key: "folder_not_found" } });
    const server = connection({ status: "needs_repair", reason: { key: "unreachable" } });
    const blocked = connection({ status: "blocked", reason: { key: "forbidden_by_policy" } });

    // WHEN the action that repairs each is asked
    // THEN it is to sign in, enter the password, choose the folder, start again, see the rule
    expect(repairAction(native)?.kind).toBe("sign_in");
    expect(repairAction(approval)?.kind).toBe("sign_in");
    expect(repairAction(mailbox)?.kind).toBe("password");
    expect(repairAction(folder)?.kind).toBe("folder");
    expect(repairAction(server)?.kind).toBe("restart");
    expect(repairAction(blocked)).toEqual({ kind: "rule", labelKey: "connections.card.repair.rule" });
  });

  test("a remote service that asks to sign in again is signed in again, not started again", () => {
    // GIVEN a remote service whose sign-in expired, and a local service in the same state
    const remote = connection({ status: "needs_repair", reason: { key: "sign_in_again" } });
    const local = connection({ remote: false, status: "needs_repair", reason: { key: "sign_in_again" } });

    // WHEN the action that repairs each is asked
    // THEN the remote one signs in again; the local one, which has no sign-in, starts again
    expect(repairAction(remote)).toEqual({ kind: "sign_in", labelKey: "connections.card.repair.sign_in" });
    expect(repairAction(local)?.kind).toBe("restart");
  });

  test("a working, paused or never checked connection offers no repair", () => {
    // GIVEN a connection in each of those states, and a mailbox that cannot be reached
    const states = [
      connection(),
      connection({ status: "paused" }),
      connection({ status: "not_checked", reason: { key: "not_checked" } }),
      connection({ kind: "imap", status: "needs_repair", reason: { key: "unreachable" } }),
    ];

    // WHEN the action that repairs each is asked
    // THEN there is none: checking again is all there is to do
    for (const c of states) expect(repairAction(c)).toBeNull();
  });
});

describe("pauseGesture", () => {
  test("a paused connection is resumed, another paused, a blocked one neither", () => {
    // GIVEN the states of a connection
    // WHEN the gesture of each is asked
    // THEN pause and resume alternate, and the organisation's block is not lifted from here
    expect(pauseGesture("working")).toBe("pause");
    expect(pauseGesture("needs_repair")).toBe("pause");
    expect(pauseGesture("paused")).toBe("resume");
    expect(pauseGesture("blocked")).toBeNull();
  });
});

describe("removalLines", () => {
  test("what a removal erases is said line by line, its kept state left out", () => {
    // GIVEN what the runtime answers a dry run of a server's removal
    const items = [
      { kind: "configuration" as const, name: "linear" },
      { kind: "secret" as const, name: "LINEAR_API_KEY" },
      { kind: "agent_access" as const, name: "mcp:linear/list_issues", agent: "triage" },
      { kind: "state" as const, name: "mcp:linear" },
    ];

    // WHEN its lines are built
    const lines = removalLines(items);

    // THEN each thing is one line, the agent named on its access, the state not listed
    expect(lines).toEqual([
      { key: "connections.card.remove.item.configuration", values: { name: "linear" } },
      { key: "connections.card.remove.item.secret", values: { name: "LINEAR_API_KEY" } },
      {
        key: "connections.card.remove.item.agent_access",
        values: { name: "mcp:linear/list_issues", agent: "triage" },
      },
    ]);
  });
});

describe("checkAndReplace", () => {
  test("a check puts the state found in the list without reading the list again", async () => {
    // GIVEN a list with a connection never checked, and a check that finds it working
    const never = connection({ id: "imap:ada@example.com", kind: "imap", status: "not_checked", reason: { key: "not_checked" } });
    const other = connection({ id: "mcp:files" });
    const list = [never, other];
    const found = { ...never, status: "working" as const, reason: null, checkedAt: "2026-10-22T08:00:00Z" };
    const check = vi.fn(async () => found);
    const readList = vi.fn(async () => list);

    // WHEN Check is clicked on its badge
    const next = await checkAndReplace(never.id, check, () => list);

    // THEN the check was asked for that connection, its row holds the state found,
    // the other row is untouched, and the list was not read again
    expect(check).toHaveBeenCalledWith(never.id);
    expect(next[0]).toEqual(found);
    expect(next[1]).toBe(other);
    expect(readList).not.toHaveBeenCalled();
  });

  test("a connection replaced keeps its place, an unknown one changes nothing", () => {
    // GIVEN two connections
    const a = connection({ id: "mcp:a" });
    const b = connection({ id: "mcp:b" });

    // WHEN b is replaced, then a connection the list does not hold
    const replaced = replaceConnection([a, b], { ...b, status: "paused" });
    const unknown = replaceConnection([a, b], connection({ id: "mcp:c" }));

    // THEN b keeps its place with its new state, and the unknown one is not added
    expect(replaced.map((c) => c.status)).toEqual(["working", "paused"]);
    expect(unknown).toEqual([a, b]);
  });
});
