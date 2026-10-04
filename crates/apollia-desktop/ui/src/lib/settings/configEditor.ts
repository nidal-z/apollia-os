// The editor of `apollia.toml` in Settings > Configuration, as a state and
// the decisions that move it: what is shown, when "Save" is allowed, what a
// change made elsewhere does to the text being edited.
//
// The shell calls are injected, so Vitest drives the whole editor with a
// simulated runtime and no rendering. The component only draws this state.

import { get, writable, type Readable } from "svelte/store";
import type {
  ConfigDiagnostic,
  ConfigFileView,
  ConfigValidation,
  LockedConfigKey,
  PendingRestartKey,
} from "$lib/ipc/configFile";
import { lineDiff, lockedLinesIntact, type DiffLine } from "$lib/settings/configText";

/** Pause after the last keystroke before the text is checked, in ms. */
export const VALIDATION_PAUSE_MS = 400;

/** The shell calls the editor makes. */
export interface ConfigEditorIpc {
  read: () => Promise<ConfigFileView>;
  validate: (text: string) => Promise<ConfigValidation>;
  save: (text: string, sha256: string) => Promise<ConfigFileView>;
  defaults: () => Promise<string>;
  restart: () => Promise<void>;
}

/** A timer the tests can run by hand. */
export interface EditorTimer {
  set: (run: () => void, ms: number) => unknown;
  clear: (handle: unknown) => void;
}

const realTimer: EditorTimer = {
  set: (run, ms) => setTimeout(run, ms),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/** The notice at the head of the editor. */
export type EditorNotice =
  | { kind: "restart" }
  | { kind: "disk-changed"; disk: ConfigFileView }
  | { kind: "reloaded" }
  | { kind: "saved" }
  | { kind: "error"; message: string };

/** The dialog over the editor. */
export type EditorDialog =
  | { kind: "defaults"; text: string; diff: DiffLine[] }
  | { kind: "compare"; diff: DiffLine[] }
  | { kind: "restart" };

export interface ConfigEditorState {
  status: "loading" | "ready" | "error";
  /** Why the file could not be read, when `status` is `error`. */
  loadError: string | null;
  path: string;
  exists: boolean;
  /** The text last read from or written to the disk. */
  baseText: string;
  /** Its fingerprint, presented when saving. */
  baseSha: string;
  /** The text being edited. */
  buffer: string;
  diagnostics: ConfigDiagnostic[];
  locked: LockedConfigKey[];
  pendingRestart: PendingRestartKey[];
  notice: EditorNotice | null;
  dialog: EditorDialog | null;
  /** A check of the buffer is awaited: its verdict is not known yet. */
  validating: boolean;
  saving: boolean;
  restarting: boolean;
}

export function initialEditorState(): ConfigEditorState {
  return {
    status: "loading",
    loadError: null,
    path: "",
    exists: false,
    baseText: "",
    baseSha: "",
    buffer: "",
    diagnostics: [],
    locked: [],
    pendingRestart: [],
    notice: null,
    dialog: null,
    validating: false,
    saving: false,
    restarting: false,
  };
}

/** The buffer differs from the file. */
export function isDirty(state: ConfigEditorState): boolean {
  return state.buffer !== state.baseText;
}

/**
 * "Save" is allowed for a changed buffer whose last check found no fault,
 * once that check has answered, and while nothing else is being written.
 */
export function canSave(state: ConfigEditorState): boolean {
  return (
    state.status === "ready" &&
    isDirty(state) &&
    !state.validating &&
    !state.saving &&
    state.diagnostics.length === 0
  );
}

/** The dotted keys the policy fixes. */
export function lockedKeyNames(state: ConfigEditorState): string[] {
  return state.locked.map((k) => k.key);
}

/** The 1-based lines that wait for a restart. */
export function pendingLines(state: ConfigEditorState): Set<number> {
  const lines = new Set<number>();
  for (const key of state.pendingRestart) if (key.line !== null) lines.add(key.line);
  return lines;
}

/** The first message of each line at fault, by 1-based line. */
export function diagnosticsByLine(state: ConfigEditorState): Map<number, string> {
  const byLine = new Map<number, string>();
  for (const d of state.diagnostics) if (!byLine.has(d.line)) byLine.set(d.line, d.message);
  return byLine;
}

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  return typeof error === "string" ? error : String(error);
}

export interface ConfigEditor extends Readable<ConfigEditorState> {
  load: () => Promise<void>;
  /** Takes a new buffer; false when it would touch a locked line (undone). */
  edit: (text: string) => boolean;
  save: () => Promise<void>;
  askDefaults: () => Promise<void>;
  confirmDefaults: () => Promise<void>;
  /** The file may have changed elsewhere: re-read it and react. */
  fileChanged: () => Promise<void>;
  reloadFromDisk: () => void;
  compareWithDisk: () => void;
  askRestart: () => void;
  confirmRestart: () => Promise<void>;
  closeDialog: () => void;
  dismissNotice: () => void;
  /** Stops the pending check, when the editor goes away. */
  dispose: () => void;
}

export function createConfigEditor(
  ipc: ConfigEditorIpc,
  timer: EditorTimer = realTimer,
): ConfigEditor {
  const store = writable<ConfigEditorState>(initialEditorState());
  let pendingCheck: unknown = null;
  let checkSeq = 0;
  let recheckAfterSave = false;

  const update = (patch: Partial<ConfigEditorState>) => store.update((s) => ({ ...s, ...patch }));

  function adopt(view: ConfigFileView): Partial<ConfigEditorState> {
    return {
      status: "ready",
      loadError: null,
      path: view.path,
      exists: view.exists,
      baseText: view.text,
      baseSha: view.sha256,
      buffer: view.text,
      diagnostics: view.diagnostics,
      locked: view.locked,
      pendingRestart: view.pendingRestart,
      validating: false,
    };
  }

  function cancelCheck(): void {
    if (pendingCheck !== null) timer.clear(pendingCheck);
    pendingCheck = null;
    checkSeq++;
  }

  async function check(seq: number, text: string): Promise<void> {
    try {
      const verdict = await ipc.validate(text);
      if (seq !== checkSeq) return;
      update({ diagnostics: verdict.diagnostics, validating: false });
    } catch (error) {
      if (seq !== checkSeq) return;
      update({ validating: false, notice: { kind: "error", message: message(error) } });
    }
  }

  async function load(): Promise<void> {
    cancelCheck();
    update({ status: "loading" });
    try {
      const view = await ipc.read();
      update({
        ...adopt(view),
        notice: view.pendingRestart.length > 0 ? { kind: "restart" } : null,
      });
    } catch (error) {
      update({ status: "error", loadError: message(error) });
    }
  }

  function edit(text: string): boolean {
    const state = get(store);
    if (!lockedLinesIntact(state.buffer, text, lockedKeyNames(state))) return false;
    cancelCheck();
    const seq = checkSeq;
    update({ buffer: text, validating: true });
    pendingCheck = timer.set(() => {
      pendingCheck = null;
      void check(seq, text);
    }, VALIDATION_PAUSE_MS);
    return true;
  }

  async function write(text: string): Promise<void> {
    const state = get(store);
    cancelCheck();
    update({ saving: true, validating: false });
    try {
      const view = await ipc.save(text, state.baseSha);
      update({
        ...adopt(view),
        notice: view.pendingRestart.length > 0 ? { kind: "restart" } : { kind: "saved" },
      });
    } catch (error) {
      await refused(state.baseSha, message(error));
    } finally {
      update({ saving: false });
    }
    if (recheckAfterSave) {
      recheckAfterSave = false;
      await fileChanged();
    }
  }

  /**
   * A refused save: when the file changed since it was read, the one notice of
   * a file changed elsewhere; otherwise the runtime's sentence.
   */
  async function refused(baseSha: string, sentence: string): Promise<void> {
    try {
      const disk = await ipc.read();
      if (disk.sha256 !== baseSha) {
        update({ notice: { kind: "disk-changed", disk } });
        return;
      }
    } catch {
      // The file cannot be read again: say why the save was refused.
    }
    update({ notice: { kind: "error", message: sentence } });
  }

  async function save(): Promise<void> {
    const state = get(store);
    if (!canSave(state)) return;
    await write(state.buffer);
  }

  async function askDefaults(): Promise<void> {
    try {
      const text = await ipc.defaults();
      const state = get(store);
      update({ dialog: { kind: "defaults", text, diff: lineDiff(state.buffer, text) } });
    } catch (error) {
      update({ notice: { kind: "error", message: message(error) } });
    }
  }

  async function confirmDefaults(): Promise<void> {
    const dialog = get(store).dialog;
    if (dialog?.kind !== "defaults") return;
    update({ dialog: null, buffer: dialog.text });
    await write(dialog.text);
  }

  async function fileChanged(): Promise<void> {
    const state = get(store);
    if (state.saving) {
      recheckAfterSave = true;
      return;
    }
    if (state.status !== "ready") return;
    let disk: ConfigFileView;
    try {
      disk = await ipc.read();
    } catch {
      return;
    }
    const now = get(store);
    if (disk.sha256 === now.baseSha) {
      update({ pendingRestart: disk.pendingRestart, locked: disk.locked });
      return;
    }
    if (!isDirty(now)) {
      cancelCheck();
      update({ ...adopt(disk), notice: { kind: "reloaded" } });
      return;
    }
    update({ notice: { kind: "disk-changed", disk } });
  }

  function reloadFromDisk(): void {
    const state = get(store);
    const disk = state.notice?.kind === "disk-changed" ? state.notice.disk : null;
    if (!disk) return;
    cancelCheck();
    update({ ...adopt(disk), notice: { kind: "reloaded" }, dialog: null });
  }

  function compareWithDisk(): void {
    const state = get(store);
    if (state.notice?.kind !== "disk-changed") return;
    update({ dialog: { kind: "compare", diff: lineDiff(state.buffer, state.notice.disk.text) } });
  }

  function askRestart(): void {
    update({ dialog: { kind: "restart" } });
  }

  async function confirmRestart(): Promise<void> {
    update({ restarting: true });
    try {
      await ipc.restart();
    } catch (error) {
      update({ notice: { kind: "error", message: message(error) } });
    } finally {
      update({ restarting: false, dialog: null });
    }
  }

  return {
    subscribe: store.subscribe,
    load,
    edit,
    save,
    askDefaults,
    confirmDefaults,
    fileChanged,
    reloadFromDisk,
    compareWithDisk,
    askRestart,
    confirmRestart,
    closeDialog: () => update({ dialog: null }),
    dismissNotice: () => update({ notice: null }),
    dispose: cancelCheck,
  };
}
