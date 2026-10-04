import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { companionStore } from "$lib/stores/companion";
import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";
import type { GuideContext } from "$lib/stores/guideContext";

/** The context of a screen with no declared sub-screen. */
function onScreen(route: string): GuideContext {
  return { route, subscreen: null, object: null };
}

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  companionStore.reset();
  detailSidebar.reset();
  vi.clearAllMocks();
  mockedInvoke.mockResolvedValue("");
});

// ── toggleCompanion ───────────────────────────────────────────────────────────

describe("companionStore - toggleCompanion", () => {
  test("opens the Guide tab and enables the companion", () => {
    // GIVEN companion disabled and the Guide tab closed
    expect(get(companionStore).enabled).toBe(false);
    expect(get(guideVisible)).toBe(false);

    // WHEN
    companionStore.toggleCompanion();

    // THEN
    expect(get(companionStore).enabled).toBe(true);
    expect(get(guideVisible)).toBe(true);
  });

  test("closes the Guide tab and disables the companion", () => {
    // GIVEN companion already enabled
    companionStore.toggleCompanion();
    expect(get(companionStore).enabled).toBe(true);

    // WHEN toggled again
    companionStore.toggleCompanion();

    // THEN both are false
    expect(get(companionStore).enabled).toBe(false);
    expect(get(guideVisible)).toBe(false);
  });

  test("persists enabled state via set_companion_enabled IPC", () => {
    // GIVEN companion disabled
    // WHEN toggled to enabled
    companionStore.toggleCompanion();

    // THEN IPC is called with enabled=true
    expect(mockedInvoke).toHaveBeenCalledWith("set_companion_enabled", {
      enabled: true,
    });
  });
});

// ── openCompanion ─────────────────────────────────────────────────────────────

describe("companionStore - openCompanion", () => {
  test("opens the detail sidebar on the Guide tab without persisting", () => {
    // GIVEN the Guide tab closed
    expect(get(guideVisible)).toBe(false);

    // WHEN the guide is opened (Cmd+Shift+C)
    companionStore.openCompanion();

    // THEN the Guide tab shows and no preference is written
    expect(get(guideVisible)).toBe(true);
    expect(mockedInvoke).not.toHaveBeenCalledWith(
      "set_companion_enabled",
      expect.anything(),
    );
  });
});

// ── updateContext ─────────────────────────────────────────────────────────────

describe("companionStore - updateContext", () => {
  test("stores fetched context text and updates currentRoute", async () => {
    // GIVEN
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_companion_context") return Promise.resolve("You are on the Agents page.");
      return Promise.resolve(null);
    });

    // WHEN
    await companionStore.updateContext(onScreen("agents"));

    // THEN
    const state = get(companionStore);
    expect(state.currentContext).toBe("You are on the Agents page.");
    expect(state.currentRoute).toBe("agents");
  });

  test("a declared sub-screen and its object reach the guide's instructions", async () => {
    // GIVEN an active session and the route text of the Agents screen
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "create_companion_session")
        return Promise.resolve({ session_id: "sess-2" });
      if (cmd === "get_companion_context")
        return Promise.resolve("Agents context.");
      return Promise.resolve(null);
    });
    await companionStore.createSession("agents");

    // WHEN the context carries the agent sheet and the agent it shows
    await companionStore.updateContext({
      route: "agents",
      subscreen: "agents.fiche",
      object: { id: "agent-7", name: "Qualify requests" },
    });

    // THEN the route text is looked up by route, and the instructions sent to
    // the session name the sub-screen and the agent
    expect(mockedInvoke).toHaveBeenCalledWith("get_companion_context", {
      route: "agents",
    });
    const call = mockedInvoke.mock.calls.find(
      ([cmd]) => cmd === "update_chat_session",
    );
    const update = (call?.[1] as { update: { system_prompt: string } }).update;
    expect(update.system_prompt).toContain("Agents context.");
    expect(update.system_prompt).toContain("agents.fiche");
    expect(update.system_prompt).toContain('"Qualify requests" (id: agent-7)');
    const state = get(companionStore);
    expect(state.currentSubscreen).toBe("agents.fiche");
    expect(state.currentObject).toEqual({ id: "agent-7", name: "Qualify requests" });
  });

  test("session id is preserved across route changes", async () => {
    // GIVEN a companion session was created
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "create_companion_session")
        return Promise.resolve({ session_id: "abc123" });
      if (cmd === "get_companion_context")
        return Promise.resolve("Context text.");
      return Promise.resolve(null);
    });
    await companionStore.createSession("agents");
    expect(get(companionStore).sessionId).toBe("abc123");

    // WHEN context is updated for a different route
    await companionStore.updateContext(onScreen("tasks"));

    // THEN the session id is unchanged
    expect(get(companionStore).sessionId).toBe("abc123");
  });

  test("updates session system prompt when session is active", async () => {
    // GIVEN an active companion session
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "create_companion_session")
        return Promise.resolve({ session_id: "sess-1" });
      if (cmd === "get_companion_context")
        return Promise.resolve("Tasks context.");
      return Promise.resolve(null);
    });
    await companionStore.createSession("agents");

    // WHEN context is updated
    await companionStore.updateContext(onScreen("tasks"));

    // THEN update_chat_session IPC is called with the new context as system
    // prompt, under the argument key Tauri actually reads. Tauri camel-cases
    // the names of command arguments, so `session_id: String` is looked up as
    // `sessionId`. The nested `update` payload stays snake_case, its Rust type
    // carrying no `rename_all`.
    expect(mockedInvoke).toHaveBeenCalledWith("update_chat_session", {
      sessionId: "sess-1",
      update: { system_prompt: "Tasks context.", tools: null, llm_backend: null },
    });
  });

  test("no argument key reaches update_chat_session in snake_case", async () => {
    // GIVEN an active companion session
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "create_companion_session")
        return Promise.resolve({ session_id: "sess-1" });
      if (cmd === "get_companion_context")
        return Promise.resolve("Agents context.");
      return Promise.resolve(null);
    });
    await companionStore.createSession("tasks");

    // WHEN context is updated
    await companionStore.updateContext(onScreen("agents"));

    // THEN the argument object carries `sessionId` and never `session_id`:
    // a key Tauri does not find makes it reject the whole call, so the wrong
    // spelling is not a default value, it is a dead surface.
    const call = mockedInvoke.mock.calls.find(
      ([cmd]) => cmd === "update_chat_session",
    );
    expect(call).toBeDefined();
    const args = call?.[1] as Record<string, unknown>;
    expect(Object.keys(args)).toContain("sessionId");
    expect(Object.keys(args)).not.toContain("session_id");
  });
});

// ── initFromMemory ────────────────────────────────────────────────────────────

describe("companionStore - initFromMemory", () => {
  test("restores the opt-in but keeps the Guide tab closed at startup", async () => {
    // GIVEN a profile where the companion was enabled
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_companion_enabled") return Promise.resolve(true);
      return Promise.resolve(undefined);
    });

    // WHEN the store initialises at launch
    await companionStore.initFromMemory();

    // THEN the feature stays enabled but the panel does not auto-open:
    // showing it on every launch spawned an empty chat session each time.
    // The user opens it on demand via the toggle or the global shortcut.
    expect(get(companionStore).enabled).toBe(true);
    expect(get(guideVisible)).toBe(false);
  });

  test("leaves companion disabled when get_companion_enabled returns false", async () => {
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_companion_enabled") return Promise.resolve(false);
      return Promise.resolve(undefined);
    });

    await companionStore.initFromMemory();

    expect(get(companionStore).enabled).toBe(false);
    expect(get(guideVisible)).toBe(false);
  });

  test("leaves companion disabled when get_companion_enabled returns undefined", async () => {
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_companion_enabled") return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });

    await companionStore.initFromMemory();

    expect(get(companionStore).enabled).toBe(false);
  });

  test("gracefully handles IPC failure without throwing", async () => {
    // GIVEN IPC fails
    mockedInvoke.mockRejectedValue(new Error("IPC error"));

    // WHEN / THEN - no error thrown, companion remains disabled
    await expect(companionStore.initFromMemory()).resolves.toBeUndefined();
    expect(get(companionStore).enabled).toBe(false);
  });
});
