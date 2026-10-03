import { describe, test, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  UPDATE_CHANNEL_KEY,
  hydrateUpdateChannel,
  setUpdateChannel,
  updateChannel,
  updateChannelSelectState,
  updateChannelSource,
  type UpdateChannelIpc,
} from "./updateChannel";
import type { ResolvedSetting, SettingSource } from "$lib/ipc/settings";

/** The shell, backed by an `apollia.toml` held in memory. */
function fakeShell(file?: string, imposed?: string) {
  const state = { file };
  const resolve = (): ResolvedSetting => {
    if (imposed !== undefined) return { key: UPDATE_CHANNEL_KEY, value: imposed, source: "policy" };
    return state.file === undefined
      ? { key: UPDATE_CHANNEL_KEY, value: "stable", source: "default" }
      : { key: UPDATE_CHANNEL_KEY, value: state.file, source: "user" };
  };
  const ipc: UpdateChannelIpc = {
    read: vi.fn(async () => resolve()),
    write: vi.fn(async (_key: string, value: unknown) => {
      if (imposed !== undefined) throw new Error("locked by the organization policy");
      state.file = value as string;
      return resolve();
    }),
  };
  return { ipc, state };
}

beforeEach(() => {
  updateChannel.set("stable");
  updateChannelSource.set("default");
});

describe("update channel selector", () => {
  test("a file without the key shows the stable channel", async () => {
    // GIVEN an apollia.toml that does not name the update channel
    const { ipc } = fakeShell();

    // WHEN the selector is seeded
    await hydrateUpdateChannel(ipc);

    // THEN it shows stable, from the default
    expect(get(updateChannel)).toBe("stable");
    expect(get(updateChannelSource)).toBe("default");
    expect(ipc.read).toHaveBeenCalledWith("desktop.update_channel");
  });

  test("choosing preview writes it to [desktop] update_channel", async () => {
    // GIVEN the stable channel in force
    const { ipc, state } = fakeShell();

    // WHEN the preview channel is chosen
    const written = await setUpdateChannel("preview", ipc);

    // THEN the write reaches the file and the selector shows preview
    expect(written).toBe(true);
    expect(ipc.write).toHaveBeenCalledWith("desktop.update_channel", "preview");
    expect(state.file).toBe("preview");
    expect(get(updateChannel)).toBe("preview");
    expect(get(updateChannelSource)).toBe("user");
  });

  test("a channel the policy imposes locks the selector and refuses a change", async () => {
    // GIVEN a policy imposing the stable channel
    const { ipc } = fakeShell(undefined, "stable");
    await hydrateUpdateChannel(ipc);

    // WHEN the preview channel is chosen anyway
    const written = await setUpdateChannel("preview", ipc);

    // THEN the selector is locked, the write refused, and stable comes back
    expect(updateChannelSelectState(get(updateChannelSource))).toEqual({
      disabled: true,
      locked: true,
    });
    expect(written).toBe(false);
    expect(get(updateChannel)).toBe("stable");
  });

  test("a value from the user or the default leaves the selector free", () => {
    // GIVEN the two sources a person controls
    const sources: SettingSource[] = ["user", "default"];

    // WHEN the selector state is computed for each
    const states = sources.map(updateChannelSelectState);

    // THEN none is locked
    for (const state of states) expect(state).toEqual({ disabled: false, locked: false });
  });

  test("an unreachable shell keeps the stable channel", async () => {
    // GIVEN a shell that cannot be reached
    const ipc: UpdateChannelIpc = {
      read: vi.fn(async () => {
        throw new Error("unreachable");
      }),
      write: vi.fn(),
    };

    // WHEN the selector is seeded
    await hydrateUpdateChannel(ipc);

    // THEN it shows stable
    expect(get(updateChannel)).toBe("stable");
  });
});
