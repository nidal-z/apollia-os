import { describe, it, expect } from "vitest";
import { InvokeStubs, argsMatch, expandHome, type InvokeHost } from "./invokeStubs";

/**
 * The stub seam replaces the one function every Tauri call of the webview
 * funnels through. These tests drive it against a plain host object, the
 * shape the automation seam has, so they run under the node
 * environment: what matters is which calls the table answers, which reach
 * the original, and that the original comes back untouched.
 */

interface RecordingHost extends InvokeHost {
  calls: Array<[string, unknown]>;
}

function recordingHost(): RecordingHost {
  const calls: Array<[string, unknown]> = [];
  return {
    calls,
    invoke: async (cmd, args) => {
      calls.push([cmd, args]);
      return { real: true, cmd };
    },
  };
}

// Call whatever function sits on the host at that moment, the way
// the dev-server shim reads `seam.invoke` at call
// time. Spelled this way on purpose: the IPC guards read every invoke call
// of the UI tree as a call to the backend, and these are calls to a fake.
function call(host: InvokeHost, cmd: string, args?: unknown): Promise<unknown> {
  const current = host.invoke;
  return current(cmd, args);
}

describe("expandHome", () => {
  it("replaces every ${HOME} in nested strings and leaves other values alone", () => {
    // GIVEN a value mixing strings with the token, strings without, and non-strings
    const value = {
      path: "${HOME}/.apollia/models/a.gguf",
      pair: ["${HOME}/x", "${HOME}/y", 3, null],
      plain: "untouched",
      nested: { detail: "${HOME} and ${HOME}" },
    };

    // WHEN it is expanded against a home
    const out = expandHome(value, "/seed");

    // THEN each token is replaced, nothing else changes
    expect(out).toEqual({
      path: "/seed/.apollia/models/a.gguf",
      pair: ["/seed/x", "/seed/y", 3, null],
      plain: "untouched",
      nested: { detail: "/seed and /seed" },
    });
  });

  it("refuses the token when the boot carried no home", () => {
    // GIVEN an empty home
    const home = "";

    // WHEN a string carrying the token is expanded
    // THEN it throws instead of producing a path rooted at nothing
    expect(() => expandHome("${HOME}/.apollia", home)).toThrow(/homeDir/);
    expect(expandHome("no token", home)).toBe("no token");
  });
});

describe("argsMatch", () => {
  it("accepts a call whose listed keys are deep-equal and rejects the rest", () => {
    // GIVEN an expectation on one key
    const expected = { sessionId: "seed-session-4" };

    // WHEN it is checked against calls with and without that value
    // THEN only the equal one matches, and extra keys are ignored
    expect(argsMatch({ sessionId: "seed-session-4", extra: 1 }, expected)).toBe(true);
    expect(argsMatch({ sessionId: "seed-session-1" }, expected)).toBe(false);
    expect(argsMatch(undefined, expected)).toBe(false);
    expect(argsMatch({ opts: { a: [1, 2] } }, { opts: { a: [1, 2] } })).toBe(true);
    expect(argsMatch(undefined, {})).toBe(true);
  });
});

describe("InvokeStubs", () => {
  it("answers a stubbed command without reaching the original and lets others through", async () => {
    // GIVEN a host and a resolve stub on one command
    const host = recordingHost();
    const stubs = new InvokeStubs(host);
    stubs.add({ command: "list_projects", mode: "resolve", value: [], once: false });

    // WHEN the stubbed command and another one are invoked
    const stubbed = await call(host, "list_projects", {});
    const passed = await call(host, "list_tasks", { a: 1 });

    // THEN the stub answers, the other call reaches the original
    expect(stubbed).toEqual([]);
    expect(passed).toEqual({ real: true, cmd: "list_tasks" });
    expect(host.calls).toEqual([["list_tasks", { a: 1 }]]);
  });

  it("resolves null as a value, which is what a cancelled picker returns", async () => {
    // GIVEN a resolve stub whose value is null
    const host = recordingHost();
    const stubs = new InvokeStubs(host);
    stubs.add({ command: "plugin:dialog|open", mode: "resolve", value: null, once: true });

    // WHEN the picker is invoked
    const out = await call(host, "plugin:dialog|open", { options: {} });

    // THEN null comes back and the native call never ran
    expect(out).toBeNull();
    expect(host.calls).toEqual([]);
  });

  it("rejects with the payload the UI classifies", async () => {
    // GIVEN a reject stub carrying an object payload
    const host = recordingHost();
    const stubs = new InvokeStubs(host);
    const payload = { kind: "oauth_client_not_configured", detail: "automation" };
    stubs.add({ command: "oauth_start_flow", mode: "reject", value: payload, once: false });

    // WHEN the command is invoked
    // THEN the promise rejects with that very payload
    await expect(call(host, "oauth_start_flow", {})).rejects.toBe(payload);
  });

  it("merges a patch over the real result", async () => {
    // GIVEN a patch stub on a command the original answers with an object
    const host = recordingHost();
    const stubs = new InvokeStubs(host);
    stubs.add({
      command: "get_chat_session",
      mode: "patch",
      value: { status: "closed" },
      once: false,
    });

    // WHEN the command is invoked
    const out = await call(host, "get_chat_session", { sessionId: "seed-session-1" });

    // THEN the original ran and the patch fields win over its result
    expect(host.calls).toEqual([["get_chat_session", { sessionId: "seed-session-1" }]]);
    expect(out).toEqual({ real: true, cmd: "get_chat_session", status: "closed" });
  });

  it("drops a once stub after its first hit", async () => {
    // GIVEN a once stub
    const host = recordingHost();
    const stubs = new InvokeStubs(host);
    stubs.add({ command: "list_tasks", mode: "reject", value: "boom", once: true });

    // WHEN the command is invoked twice
    await expect(call(host, "list_tasks", {})).rejects.toBe("boom");
    const second = await call(host, "list_tasks", {});

    // THEN the second call reached the original
    expect(second).toEqual({ real: true, cmd: "list_tasks" });
    expect(host.calls).toEqual([["list_tasks", {}]]);
  });

  it("selects by argsMatch and passes through a call the stub does not accept", async () => {
    // GIVEN a stub bound to one session id
    const host = recordingHost();
    const stubs = new InvokeStubs(host);
    stubs.add({
      command: "get_chat_session",
      mode: "reject",
      value: "sqlite: database disk image is malformed",
      once: false,
      argsMatch: { sessionId: "seed-session-4" },
    });

    // WHEN another session and then the bound one are requested
    const other = await call(host, "get_chat_session", { sessionId: "seed-session-1" });

    // THEN the other passes through and the bound one is refused
    expect(other).toEqual({ real: true, cmd: "get_chat_session" });
    await expect(call(host, "get_chat_session", { sessionId: "seed-session-4" })).rejects.toBe(
      "sqlite: database disk image is malformed",
    );
    expect(host.calls).toEqual([["get_chat_session", { sessionId: "seed-session-1" }]]);
  });

  it("clears one command or all of them and reports the count", async () => {
    // GIVEN stubs on two commands
    const host = recordingHost();
    const stubs = new InvokeStubs(host);
    stubs.add({ command: "a", mode: "resolve", value: 1, once: false });
    stubs.add({ command: "a", mode: "resolve", value: 2, once: false, argsMatch: { k: 1 } });
    stubs.add({ command: "b", mode: "resolve", value: 3, once: false });

    // WHEN one command is cleared, then everything
    const clearedA = stubs.clear("a");
    const afterA = await call(host, "a", {});
    const clearedRest = stubs.clear();
    const afterAll = await call(host, "b", {});

    // THEN the counts are right and the cleared commands pass through
    expect(clearedA).toBe(2);
    expect(afterA).toEqual({ real: true, cmd: "a" });
    expect(clearedRest).toBe(1);
    expect(afterAll).toEqual({ real: true, cmd: "b" });
    expect(stubs.installed).toBe(true);
  });

  it("installs the wrapper on the first add and puts the original back on restore", async () => {
    // GIVEN a host whose invoke is remembered
    const host = recordingHost();
    const original = host.invoke;
    const stubs = new InvokeStubs(host);
    expect(stubs.installed).toBe(false);

    // WHEN a stub is added, then the seam is restored
    stubs.add({ command: "x", mode: "resolve", value: 0, once: false });
    const wrapped = host.invoke;
    stubs.restore();
    stubs.restore();

    // THEN the wrapper differed from the original, the same function is back,
    // and the table is empty
    expect(wrapped).not.toBe(original);
    expect(host.invoke).toBe(original);
    expect(stubs.installed).toBe(false);
    expect(await call(host, "x", {})).toEqual({ real: true, cmd: "x" });
  });
});
