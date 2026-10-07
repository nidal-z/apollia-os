import { vi, describe, test, expect, beforeAll, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { get } from "svelte/store";
import { locale, t, waitLocale } from "svelte-i18n";
import "$lib/i18n";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { invoke } from "@tauri-apps/api/core";
import { companionStore } from "$lib/stores/companion";
import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";
import type { GuideContext } from "$lib/stores/guideContext";

/** The context of a screen with no declared sub-screen. */
function onScreen(route: string): GuideContext {
  return { route, subscreen: null, object: null };
}

const mockedInvoke = vi.mocked(invoke);

// The context text is a catalogue entry, read in the language of the interface.
beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

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
  test("stores the catalogue text of the route and updates currentRoute", async () => {
    // GIVEN the interface in English
    // WHEN the guide is given the Agents screen
    await companionStore.updateContext(onScreen("agents"));

    // THEN the context is the catalogue text of that screen, with the action
    // the guide may offer and the route it opens
    const state = get(companionStore);
    expect(state.currentContext).toContain(get(t)("guide.context.agents.text"));
    expect(state.currentContext).toContain("(/chat)");
    expect(state.currentRoute).toBe("agents");
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  test("the context text follows the language of the interface", async () => {
    // GIVEN the Journal screen described in English
    await companionStore.updateContext(onScreen("journal"));
    const english = get(companionStore).currentContext;

    // WHEN the interface switches to French and the context is handed again
    locale.set("fr");
    await waitLocale();
    await companionStore.updateContext(onScreen("journal"));
    const french = get(companionStore).currentContext;
    locale.set("en");
    await waitLocale();

    // THEN the same screen is described in French
    expect(english).toContain(en.guide.context.journal.text);
    expect(french).toContain(fr.guide.context.journal.text);
    expect(fr.guide.context.journal.text).not.toBe(en.guide.context.journal.text);
  });

  test("a declared sub-screen and its object reach the guide's instructions", async () => {
    // GIVEN an active session
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "create_companion_session")
        return Promise.resolve({ session_id: "sess-2" });
      return Promise.resolve(null);
    });
    await companionStore.createSession("agents");

    // WHEN the context carries the agent sheet and the agent it shows
    await companionStore.updateContext({
      route: "agents",
      subscreen: "agents.fiche",
      object: { id: "agent-7", name: "Qualify requests" },
    });

    // THEN the instructions sent to the session carry the text of the sheet,
    // and name the sub-screen and the agent
    const call = mockedInvoke.mock.calls.find(
      ([cmd]) => cmd === "update_chat_session",
    );
    const update = (call?.[1] as { update: { system_prompt: string } }).update;
    expect(update.system_prompt).toContain("You are reading the sheet of Qualify requests.");
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
      return Promise.resolve(null);
    });
    await companionStore.createSession("agents");

    // WHEN context is updated
    await companionStore.updateContext(onScreen("journal"));

    // THEN update_chat_session IPC is called with the new context as system
    // prompt, under the argument key Tauri actually reads. Tauri camel-cases
    // the names of command arguments, so `session_id: String` is looked up as
    // `sessionId`. The nested `update` payload stays snake_case, its Rust type
    // carrying no `rename_all`.
    expect(mockedInvoke).toHaveBeenCalledWith("update_chat_session", {
      sessionId: "sess-1",
      update: {
        system_prompt: get(companionStore).currentContext,
        tools: null,
        llm_backend: null,
      },
    });
  });

  test("no argument key reaches update_chat_session in snake_case", async () => {
    // GIVEN an active companion session
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "create_companion_session")
        return Promise.resolve({ session_id: "sess-1" });
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
