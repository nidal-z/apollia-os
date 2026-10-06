/**
 * The editor of `apollia.toml`, driven through its state with a simulated
 * runtime: the check at each pause in typing, the save with the fingerprint
 * read, the default values, the lines the policy locks, the keys waiting for a
 * restart, and a file changed elsewhere.
 */
import { describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  VALIDATION_PAUSE_MS,
  canSave,
  createConfigEditor,
  diagnosticsByLine,
  pendingLines,
  type ConfigEditorIpc,
  type EditorTimer,
} from "$lib/settings/configEditor";
import { lockedLines } from "$lib/settings/configText";
import type { ConfigDiagnostic, ConfigFileView } from "$lib/ipc/configFile";

const TEXT = [
  "# apollia.toml",
  "[api]",
  "port = 7771",
  "",
  "[chat]",
  "plan_mode_default = false",
  "",
  "[desktop]",
  "developer_mode = true",
  "",
  "[tools]",
  "max_parallel = 4",
  "",
].join("\n");

function view(text: string, sha: string, extra: Partial<ConfigFileView> = {}): ConfigFileView {
  return {
    path: "/home/u/.apollia/apollia.toml",
    exists: true,
    text,
    sha256: sha,
    diagnostics: [],
    locked: [],
    pendingRestart: [],
    ...extra,
  };
}

/** A timer whose pending runs are played by hand. */
function manualTimer(): EditorTimer & { flush: () => void; delays: number[] } {
  let pending: Array<() => void> = [];
  const delays: number[] = [];
  return {
    delays,
    set: (run, ms) => {
      delays.push(ms);
      pending.push(run);
      return run;
    },
    clear: (handle) => {
      pending = pending.filter((run) => run !== handle);
    },
    flush: () => {
      const runs = pending;
      pending = [];
      for (const run of runs) run();
    },
  };
}

/** The runtime held in memory: its file, its verdicts, its refusals. */
function fakeRuntime(initial: ConfigFileView) {
  const disk = { view: initial, writes: [] as Array<{ text: string; sha: string }> };
  const ipc: ConfigEditorIpc = {
    read: vi.fn(async () => disk.view),
    validate: vi.fn(async (text: string) => {
      const diagnostics: ConfigDiagnostic[] = text.includes('port = "x"')
        ? [{ line: 12, column: 8, path: "api.port", message: "invalid type: string, expected u16" }]
        : [];
      return { valid: diagnostics.length === 0, diagnostics };
    }),
    save: vi.fn(async (text: string, sha: string) => {
      if (sha !== disk.view.sha256) throw new Error("apollia.toml changed since it was read");
      disk.writes.push({ text, sha });
      disk.view = { ...disk.view, text, sha256: `sha-${disk.writes.length}` };
      return disk.view;
    }),
    defaults: vi.fn(async () => "# defaults\n[desktop]\ndeveloper_mode = true\n"),
    restart: vi.fn(async () => {}),
  };
  return { disk, ipc };
}

async function settle(): Promise<void> {
  for (let k = 0; k < 5; k++) await Promise.resolve();
}

describe("the check at each pause in typing", () => {
  it("marks the faulty line 12 with its message and disables Save", async () => {
    // GIVEN an editor on a valid file, the runtime refusing a port written as text
    const timer = manualTimer();
    const { ipc } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, timer);
    await editor.load();
    const typed = TEXT.replace("max_parallel = 4", 'port = "x"');

    // WHEN a key is typed, then the pause passes and the runtime answers
    editor.edit(typed);
    const whileTyping = canSave(get(editor));
    timer.flush();
    await settle();

    // THEN the check waited for the pause, line 12 carries the fault, Save is off
    expect(whileTyping).toBe(false);
    expect(timer.delays).toContain(VALIDATION_PAUSE_MS);
    expect(ipc.validate).toHaveBeenCalledWith(typed);
    expect(diagnosticsByLine(get(editor)).get(12)).toBe("invalid type: string, expected u16");
    expect(canSave(get(editor))).toBe(false);
  });

  it("enables Save once the check of a changed text finds nothing", async () => {
    // GIVEN an editor on a valid file
    const timer = manualTimer();
    const { ipc } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, timer);
    await editor.load();

    // WHEN a valid change is typed and checked
    editor.edit(TEXT.replace("plan_mode_default = false", "plan_mode_default = true"));
    timer.flush();
    await settle();

    // THEN Save is on, and only one check ran for the last text
    expect(canSave(get(editor))).toBe(true);
    expect(ipc.validate).toHaveBeenCalledTimes(1);
  });

  it("checks only the last text of a burst of keystrokes", async () => {
    // GIVEN an editor on a valid file
    const timer = manualTimer();
    const { ipc } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, timer);
    await editor.load();

    // WHEN three keystrokes come before the pause
    editor.edit(`${TEXT}a`);
    editor.edit(`${TEXT}ab`);
    editor.edit(`${TEXT}abc`);
    timer.flush();
    await settle();

    // THEN one check ran, on the last text
    expect(ipc.validate).toHaveBeenCalledTimes(1);
    expect(ipc.validate).toHaveBeenCalledWith(`${TEXT}abc`);
  });
});

describe("Save", () => {
  it("saves with the fingerprint read, then the margin marks clear", async () => {
    // GIVEN an editor whose file was read with fingerprint sha-0
    const timer = manualTimer();
    const { ipc, disk } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, timer);
    await editor.load();
    const changed = TEXT.replace("plan_mode_default = false", "plan_mode_default = true");
    editor.edit(changed);
    timer.flush();
    await settle();

    // WHEN Save is pressed
    await editor.save();

    // THEN the text went with sha-0, without a dialog, and is the new base
    expect(disk.writes).toEqual([{ text: changed, sha: "sha-0" }]);
    const state = get(editor);
    expect(state.dialog).toBeNull();
    expect(state.baseSha).toBe("sha-1");
    expect(state.baseText).toBe(changed);
    expect(state.notice).toEqual({ kind: "saved" });
  });

  it("answers a file modified in the meantime with the one notice, never an overwrite", async () => {
    // GIVEN an editor with a change, the file then rewritten elsewhere
    const timer = manualTimer();
    const { ipc, disk } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, timer);
    await editor.load();
    editor.edit(TEXT.replace("port = 7771", "port = 7772"));
    timer.flush();
    await settle();
    disk.view = view(`${TEXT}# elsewhere\n`, "sha-other");

    // WHEN Save is pressed
    await editor.save();

    // THEN nothing was written and the notice offers Compare and Reload
    expect(disk.writes).toEqual([]);
    const notice = get(editor).notice;
    expect(notice?.kind).toBe("disk-changed");
    expect(get(editor).buffer).toBe(TEXT.replace("port = 7771", "port = 7772"));
  });
});

describe("Restore default values", () => {
  it("shows the comparison and writes only after the confirmation", async () => {
    // GIVEN an editor on a file that differs from the defaults
    const timer = manualTimer();
    const { ipc, disk } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, timer);
    await editor.load();

    // WHEN the defaults are asked
    await editor.askDefaults();

    // THEN the dialog shows what would change, and nothing is written yet
    const dialog = get(editor).dialog;
    expect(dialog?.kind).toBe("defaults");
    if (dialog?.kind !== "defaults") throw new Error("no dialog");
    expect(dialog.diff.some((l) => l.kind === "removed" && l.text === "port = 7771")).toBe(true);
    expect(dialog.diff.some((l) => l.kind === "added" && l.text === "# defaults")).toBe(true);
    expect(disk.writes).toEqual([]);

    // WHEN the user confirms
    await editor.confirmDefaults();

    // THEN the default text is written with the fingerprint read
    expect(disk.writes).toEqual([
      { text: "# defaults\n[desktop]\ndeveloper_mode = true\n", sha: "sha-0" },
    ]);
    expect(get(editor).dialog).toBeNull();
  });

  it("writes nothing when the dialog is closed", async () => {
    // GIVEN the defaults dialog open
    const { ipc, disk } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, manualTimer());
    await editor.load();
    await editor.askDefaults();

    // WHEN it is closed
    editor.closeDialog();

    // THEN nothing was written and the text is unchanged
    expect(disk.writes).toEqual([]);
    expect(get(editor).buffer).toBe(TEXT);
  });
});

describe("a key the policy fixes", () => {
  const locked = [{ key: "desktop.developer_mode", value: true }];

  it("locks its line, and no keystroke changes it", async () => {
    // GIVEN a file whose developer mode the policy fixes
    const { ipc } = fakeRuntime(view(TEXT, "sha-0", { locked }));
    const editor = createConfigEditor(ipc, manualTimer());
    await editor.load();

    // WHEN the line is read, then typed on, removed, and duplicated
    const lines = lockedLines(TEXT, ["desktop.developer_mode"]);
    const typedOn = editor.edit(TEXT.replace("developer_mode = true", "developer_mode = false"));
    const removed = editor.edit(TEXT.replace("developer_mode = true\n", ""));
    const added = editor.edit(TEXT.replace("[desktop]\n", "[desktop]\ndeveloper_mode = false\n"));

    // THEN line 9 is locked and every edit of it is undone
    expect([...lines]).toEqual([9]);
    expect([typedOn, removed, added]).toEqual([false, false, false]);
    expect(get(editor).buffer).toBe(TEXT);
  });

  it("leaves the other lines free", async () => {
    // GIVEN the same locked file
    const { ipc } = fakeRuntime(view(TEXT, "sha-0", { locked }));
    const editor = createConfigEditor(ipc, manualTimer());
    await editor.load();

    // WHEN another line is changed, and a line is added after the locked one
    const other = editor.edit(TEXT.replace("port = 7771", "port = 7772"));
    const after = editor.edit(
      TEXT.replace("port = 7771", "port = 7772").replace("developer_mode = true\n", "developer_mode = true\n\n"),
    );

    // THEN both are taken
    expect([other, after]).toEqual([true, true]);
  });
});

describe("keys waiting for a restart", () => {
  it("marks their line after a save, offers the restart, and restarts once confirmed", async () => {
    // GIVEN a runtime that answers a save with [api] port waiting for a restart
    const timer = manualTimer();
    const { ipc, disk } = fakeRuntime(view(TEXT, "sha-0"));
    ipc.save = vi.fn(async (text: string) => {
      disk.view = view(text, "sha-1", { pendingRestart: [{ key: "api.port", line: 3 }] });
      return disk.view;
    });
    const editor = createConfigEditor(ipc, timer);
    await editor.load();
    editor.edit(TEXT.replace("port = 7771", "port = 7772"));
    timer.flush();
    await settle();

    // WHEN the change is saved
    await editor.save();

    // THEN line 3 is marked and the notice offers the restart
    expect([...pendingLines(get(editor))]).toEqual([3]);
    expect(get(editor).notice).toEqual({ kind: "restart" });

    // WHEN the restart is asked, then confirmed
    editor.askRestart();
    const asked = get(editor).dialog;
    await editor.confirmRestart();

    // THEN a confirmation came first, then Apollia restarted once
    expect(asked).toEqual({ kind: "restart" });
    expect(ipc.restart).toHaveBeenCalledTimes(1);
  });

  it("does not restart when the confirmation is closed", async () => {
    // GIVEN the restart dialog open
    const { ipc } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, manualTimer());
    await editor.load();
    editor.askRestart();

    // WHEN it is closed
    editor.closeDialog();

    // THEN nothing restarted
    expect(ipc.restart).not.toHaveBeenCalled();
  });
});

describe("a file changed elsewhere", () => {
  it("reloads an untouched text, with a notice", async () => {
    // GIVEN an editor with no change in progress
    const { ipc, disk } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, manualTimer());
    await editor.load();
    const outside = TEXT.replace("plan_mode_default = false", "plan_mode_default = true");
    disk.view = view(outside, "sha-outside");

    // WHEN the runtime says the file was applied (another editor, another screen)
    await editor.fileChanged();

    // THEN the text is the disk's, with the reloaded notice
    expect(get(editor).buffer).toBe(outside);
    expect(get(editor).baseSha).toBe("sha-outside");
    expect(get(editor).notice).toEqual({ kind: "reloaded" });
  });

  it("keeps a changed text, says the file changed, and compares or reloads on demand", async () => {
    // GIVEN an editor with a change in progress, the file changed elsewhere
    const { ipc, disk } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, manualTimer());
    await editor.load();
    const mine = TEXT.replace("port = 7771", "port = 7772");
    editor.edit(mine);
    const outside = TEXT.replace("max_parallel = 4", "max_parallel = 8");
    disk.view = view(outside, "sha-outside");

    // WHEN the change is detected
    await editor.fileChanged();

    // THEN the text being edited is intact and the notice says the file changed
    expect(get(editor).buffer).toBe(mine);
    expect(get(editor).notice?.kind).toBe("disk-changed");

    // WHEN Compare, then Reload
    editor.compareWithDisk();
    const compared = get(editor).dialog;
    editor.reloadFromDisk();

    // THEN the comparison set the two texts side by side, and Reload took the disk's
    expect(compared?.kind).toBe("compare");
    if (compared?.kind !== "compare") throw new Error("no comparison");
    expect(compared.diff.some((l) => l.kind === "added" && l.text === "max_parallel = 8")).toBe(true);
    expect(get(editor).buffer).toBe(outside);
    expect(disk.writes).toEqual([]);
  });

  it("ignores the event of its own save", async () => {
    // GIVEN an editor that saved a change
    const timer = manualTimer();
    const { ipc } = fakeRuntime(view(TEXT, "sha-0"));
    const editor = createConfigEditor(ipc, timer);
    await editor.load();
    editor.edit(TEXT.replace("port = 7771", "port = 7772"));
    timer.flush();
    await settle();
    await editor.save();

    // WHEN the runtime's event of that save arrives
    await editor.fileChanged();

    // THEN no notice of a change elsewhere appears
    expect(get(editor).notice).toEqual({ kind: "saved" });
  });
});
