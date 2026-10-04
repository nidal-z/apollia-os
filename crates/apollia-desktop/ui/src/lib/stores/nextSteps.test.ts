import { vi, describe, test, expect, beforeEach } from "vitest";

// Minimal localStorage shim - the default vitest env is `node`, but the
// store uses `localStorage` for dismiss/feedback persistence.
class MemoryStorage {
  private readonly s = new Map<string, string>();
  getItem(k: string) {
    return this.s.has(k) ? (this.s.get(k) as string) : null;
  }
  setItem(k: string, v: string) {
    this.s.set(k, v);
  }
  removeItem(k: string) {
    this.s.delete(k);
  }
  clear() {
    this.s.clear();
  }
  key(i: number) {
    return Array.from(this.s.keys())[i] ?? null;
  }
  get length() {
    return this.s.size;
  }
}
(globalThis as unknown as { localStorage: Storage }).localStorage =
  new MemoryStorage() as unknown as Storage;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { nextSteps, GLOBAL_SCOPE, sessionScope } from "./nextSteps";
import { developerMode } from "./developerMode";

const mockedInvoke = vi.mocked(invoke);

function sampleSteps() {
  return [
    {
      id: "capture-note",
      title: "Save a note",
      description: "Keep a trace.",
      actionButton: {
        label: "Open memory",
        action: "navigate" as const,
        payload: { route: "/memory?new" },
      },
    },
    {
      id: "create-automation",
      title: "Create an automation",
      description: "Recurring routine.",
      actionButton: {
        label: "Automations",
        action: "navigate" as const,
        payload: { route: "/automations?wizard=open" },
      },
    },
    {
      id: "ask-apollia",
      title: "Ask Apollia",
      description: "Productivity coach.",
      actionButton: {
        label: "Ouvrir Apollia",
        action: "navigate" as const,
        payload: { route: "/chat" },
      },
    },
  ];
}

beforeEach(() => {
  nextSteps.reset();
  developerMode.set(false);
  localStorage.clear();
  vi.clearAllMocks();
});

describe("nextSteps store - load() surfaces 3 cards", () => {
  test("3 LLM outputs are stored and exposed via visible()", async () => {
    mockedInvoke.mockResolvedValue({ steps: sampleSteps(), fromLlm: true });

    nextSteps.load(GLOBAL_SCOPE, "global_context", {});
    await vi.waitFor(() => {
      expect(get(nextSteps.visible(GLOBAL_SCOPE)).steps).toHaveLength(3);
    });

    const view = get(nextSteps.visible(GLOBAL_SCOPE));
    expect(view.fromLlm).toBe(true);
    expect(view.steps.map((s) => s.id)).toEqual([
      "capture-note",
      "create-automation",
      "ask-apollia",
    ]);
  });
});

describe("nextSteps store - dismiss persistence", () => {
  test("dismissed card is filtered out and persisted in localStorage", async () => {
    mockedInvoke.mockResolvedValue({ steps: sampleSteps(), fromLlm: true });
    nextSteps.load(GLOBAL_SCOPE, "global_context", {});
    await vi.waitFor(() => {
      expect(get(nextSteps.visible(GLOBAL_SCOPE)).steps).toHaveLength(3);
    });

    nextSteps.dismiss("capture-note");

    const after = get(nextSteps.visible(GLOBAL_SCOPE));
    expect(after.steps.map((s) => s.id)).toEqual([
      "create-automation",
      "ask-apollia",
    ]);

    const raw = localStorage.getItem("apollia.reminders.next_steps");
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw as string);
    expect(parsed["capture-note"]).toBeGreaterThan(Date.now());
  });
});

describe("nextSteps store - fallback path", () => {
  test("fromLlm=false surfaces through the store when backend falls back", async () => {
    mockedInvoke.mockResolvedValue({
      steps: sampleSteps().slice(0, 2),
      fromLlm: false,
    });

    nextSteps.load(GLOBAL_SCOPE, "global_context", {});
    await vi.waitFor(() => {
      expect(get(nextSteps.visible(GLOBAL_SCOPE)).steps.length).toBeGreaterThan(
        0,
      );
    });

    const view = get(nextSteps.visible(GLOBAL_SCOPE));
    expect(view.fromLlm).toBe(false);
  });

  test("invoke rejection sets error and keeps steps empty", async () => {
    mockedInvoke.mockRejectedValue(new Error("LLM offline"));

    nextSteps.load(GLOBAL_SCOPE, "global_context", {});
    await vi.waitFor(() => {
      expect(get(nextSteps.visible(GLOBAL_SCOPE)).error).toBe("LLM offline");
    });

    const view = get(nextSteps.visible(GLOBAL_SCOPE));
    expect(view.steps).toHaveLength(0);
  });
});

describe("nextSteps store - feedback is local only", () => {
  test("setFeedback persists and feedbackFor returns the value", () => {
    nextSteps.setFeedback("capture-note", "useful");
    expect(nextSteps.feedbackFor("capture-note")).toBe("useful");

    const raw = localStorage.getItem("apollia.next_steps.feedback");
    expect(raw).not.toBeNull();
    expect(JSON.parse(raw as string)["capture-note"]).toBe("useful");
  });
});

describe("nextSteps store - the request follows the developer mode", () => {
  /** The `developerMode` field of every request sent so far. */
  function sentModes(): unknown[] {
    return mockedInvoke.mock.calls
      .filter(([command]) => command === "meta_generate_next_steps")
      .map(([, args]) => (args as { request: { developerMode: unknown } }).request.developerMode);
  }

  test("developer mode off asks for the default audience", async () => {
    // GIVEN the developer mode off
    mockedInvoke.mockResolvedValue({ steps: sampleSteps(), fromLlm: true });
    const scope = sessionScope("s-off");

    // WHEN the session-end panel loads
    nextSteps.load(scope, "session_end", {});
    await vi.waitFor(() => {
      expect(nextSteps.peekScope(scope).generatedAt).toBeGreaterThan(0);
    });

    // THEN the request says so, and names no former interface mode
    expect(sentModes()).toEqual([false]);
    const [, args] = mockedInvoke.mock.calls[0];
    expect((args as { request: object }).request).not.toHaveProperty("mode");
  });

  test("developer mode on asks for the developer audience", async () => {
    // GIVEN the developer mode on
    mockedInvoke.mockResolvedValue({ steps: sampleSteps(), fromLlm: true });
    developerMode.set(true);
    const scope = sessionScope("s-on");

    // WHEN the session-end panel loads
    nextSteps.load(scope, "session_end", {});
    await vi.waitFor(() => {
      expect(nextSteps.peekScope(scope).generatedAt).toBeGreaterThan(0);
    });

    // THEN the request carries the developer mode
    expect(sentModes()).toEqual([true]);
  });

  test("a cached global answer is not served to the other audience", async () => {
    // GIVEN global cards generated with the developer mode off
    mockedInvoke.mockResolvedValue({ steps: sampleSteps(), fromLlm: true });
    nextSteps.load(GLOBAL_SCOPE, "global_context", {});
    await vi.waitFor(() => {
      expect(nextSteps.peekScope(GLOBAL_SCOPE).generatedAt).toBeGreaterThan(0);
    });

    // WHEN the same panel loads again, first unchanged, then with the mode on
    nextSteps.load(GLOBAL_SCOPE, "global_context", {});
    developerMode.set(true);
    nextSteps.load(GLOBAL_SCOPE, "global_context", {});
    await vi.waitFor(() => {
      expect(sentModes()).toHaveLength(2);
    });

    // THEN the unchanged load was served from the cache, the flipped one asked again
    expect(sentModes()).toEqual([false, true]);
  });
});
