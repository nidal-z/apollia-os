import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { companionStore } from "./companion";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  companionStore.reset();
  vi.clearAllMocks();
  mockedInvoke.mockResolvedValue("");
});

// ── toggleCompanion ───────────────────────────────────────────────────────────

describe("companionStore - toggleCompanion", () => {
  test("flips enabled and visible from false to true", () => {
    // GIVEN companion disabled (enabled=false, visible=false)
    const before = get(companionStore);
    expect(before.enabled).toBe(false);
    expect(before.visible).toBe(false);

    // WHEN
    companionStore.toggleCompanion();

    // THEN
    const after = get(companionStore);
    expect(after.enabled).toBe(true);
    expect(after.visible).toBe(true);
    expect(after.minimized).toBe(false);
  });

  test("flips enabled and visible from true to false", () => {
    // GIVEN companion already enabled
    companionStore.toggleCompanion();
    expect(get(companionStore).enabled).toBe(true);

    // WHEN toggled again
    companionStore.toggleCompanion();

    // THEN both are false
    const state = get(companionStore);
    expect(state.enabled).toBe(false);
    expect(state.visible).toBe(false);
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

// ── toggleVisibility ──────────────────────────────────────────────────────────

describe("companionStore - toggleVisibility", () => {
  test("flips visible without changing enabled", () => {
    // GIVEN companion enabled and visible
    companionStore.toggleCompanion(); // enabled=true, visible=true

    // WHEN keyboard shortcut hides
    companionStore.toggleVisibility();

    // THEN visible=false but enabled stays true
    const state = get(companionStore);
    expect(state.visible).toBe(false);
    expect(state.enabled).toBe(true);
  });

  test("does not call set_companion_enabled IPC", () => {
    // GIVEN companion enabled
    companionStore.toggleCompanion();
    vi.clearAllMocks();

    // WHEN visibility is toggled via keyboard shortcut
    companionStore.toggleVisibility();

    // THEN no IPC persistence call is made
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
      if (cmd === "get_companion_context") return Promise.resolve("Vous êtes sur la page Agents.");
      return Promise.resolve(null);
    });

    // WHEN
    await companionStore.updateContext("agents");

    // THEN
    const state = get(companionStore);
    expect(state.currentContext).toBe("Vous êtes sur la page Agents.");
    expect(state.currentRoute).toBe("agents");
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
    await companionStore.updateContext("tasks");

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
    await companionStore.updateContext("tasks");

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
    await companionStore.updateContext("agents");

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
  test("restores the opt-in but keeps the panel closed at startup", async () => {
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
    const state = get(companionStore);
    expect(state.enabled).toBe(true);
    expect(state.visible).toBe(false);
  });

  test("leaves companion disabled when get_companion_enabled returns false", async () => {
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_companion_enabled") return Promise.resolve(false);
      return Promise.resolve(undefined);
    });

    await companionStore.initFromMemory();

    const state = get(companionStore);
    expect(state.enabled).toBe(false);
    expect(state.visible).toBe(false);
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
