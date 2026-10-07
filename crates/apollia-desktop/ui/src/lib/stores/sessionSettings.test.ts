import { describe, test, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { withReasoningEffort, withThinking } from "$lib/chat/composerCapabilities";
import {
  changeSessionSettings,
  loadSessionSettings,
  sessionSettingsStore,
} from "./sessionSettings";
import type { SessionSettings } from "$lib/types";

/**
 * A runtime that keeps the settings of each conversation and replaces them
 * whole on a write, as `PUT /api/v1/sessions/{id}/settings` does.
 */
function runtime(initial: Record<string, SessionSettings>) {
  const stored: Record<string, SessionSettings> = { ...initial };
  invoke.mockImplementation(async (command: string, args: Record<string, unknown>) => {
    const id = args.sessionId as string;
    if (command === "get_chat_session_settings") return stored[id] ?? {};
    if (command === "set_chat_session_settings") {
      stored[id] = args.settings as SessionSettings;
      return stored[id];
    }
    throw new Error(`unexpected command ${command}`);
  });
  return stored;
}

function writes(): Array<Record<string, unknown>> {
  return invoke.mock.calls
    .filter(([command]) => command === "set_chat_session_settings")
    .map(([, args]) => args as Record<string, unknown>);
}

beforeEach(() => {
  invoke.mockReset();
});

describe("the settings of a conversation, written from a chip", () => {
  test("switching reasoning off writes `thinking` to that conversation alone", async () => {
    // GIVEN two conversations, the first with a temperature of its own
    const stored = runtime({ "s-1": { temperature: 0.2 }, "s-2": {} });

    // WHEN the reasoning chip of the first is switched off
    await changeSessionSettings("s-1", (current) => withThinking(current, false));

    // THEN one write went to the first conversation, with its temperature kept
    expect(writes()).toEqual([
      { sessionId: "s-1", settings: { temperature: 0.2, thinking: false } },
    ]);
    // AND the other conversation is untouched
    expect(stored["s-2"]).toEqual({});
    // AND what the interface shows for the first follows the write
    expect(get(sessionSettingsStore("s-1"))).toEqual({ temperature: 0.2, thinking: false });
    expect(get(sessionSettingsStore("s-2"))).toBeNull();
  });

  test("a level is written as `reasoning_effort`, and the default removes it", async () => {
    // GIVEN a conversation that set nothing
    runtime({ "s-3": {} });

    // WHEN the middle level is chosen, then the default of the model
    await changeSessionSettings("s-3", (current) => withReasoningEffort(current, "medium"));
    await changeSessionSettings("s-3", (current) => withReasoningEffort(current, null));

    // THEN the level was sent, then a record without the field
    expect(writes().map((w) => w.settings)).toEqual([{ reasoning_effort: "medium" }, {}]);
    expect(get(sessionSettingsStore("s-3"))).toEqual({});
  });

  test("a write starts from what the runtime holds, not from what was shown", async () => {
    // GIVEN settings read once, then changed elsewhere since
    const stored = runtime({ "s-4": {} });
    await loadSessionSettings("s-4");
    stored["s-4"] = { instructions: "Answer in one line." };

    // WHEN the reasoning chip is switched off
    await changeSessionSettings("s-4", (current) => withThinking(current, false));

    // THEN the instructions written meanwhile are part of the write
    expect(writes()[0].settings).toEqual({
      instructions: "Answer in one line.",
      thinking: false,
    });
  });

  test("a refused write throws and leaves what is shown as it was", async () => {
    // GIVEN a closed conversation: its settings are read, and no longer written
    invoke.mockImplementation(async (command: string) => {
      if (command === "get_chat_session_settings") return { thinking: true };
      throw new Error("session closed");
    });
    await loadSessionSettings("s-5");

    // WHEN the reasoning chip is switched off
    const attempt = changeSessionSettings("s-5", (current) => withThinking(current, false));

    // THEN the refusal reaches the chip, and the shown setting did not move
    await expect(attempt).rejects.toThrow("session closed");
    expect(get(sessionSettingsStore("s-5"))).toEqual({ thinking: true });
  });
});
