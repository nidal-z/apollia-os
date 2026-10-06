/**
 * The live settings of the conversation the Settings tab shows.
 *
 * Every control writes at once: the settings are replaced on the runtime and
 * read by the next message, with no "Save". A refused write puts the control
 * back and says why. The notes are stored as they are typed, a moment after
 * the last key.
 */
import {
  choosePreset,
  chosenPreset,
  discardModifications,
  emptyPresetDraft,
  modifiedFields,
  parsePresetFile,
  presetDraftFrom,
  presetsOfModel,
  revertField,
  sessionModelOf,
  withSetting,
  type PresetDraft,
  type PresetField,
  type PresetFileError,
  type SessionModel,
  type SessionPreset,
  type SessionSettings,
} from "$lib/chat/sessionSettings";

/** What the settings of a conversation need from the runtime. */
export interface SessionSettingsEffects {
  getSettings: (sessionId: string) => Promise<SessionSettings>;
  setSettings: (sessionId: string, settings: SessionSettings) => Promise<SessionSettings>;
  getNotes: (sessionId: string) => Promise<string>;
  setNotes: (sessionId: string, notes: string) => Promise<string>;
  listPresets: () => Promise<SessionPreset[]>;
  createPreset: (draft: PresetDraft) => Promise<SessionPreset>;
  capabilities: (sessionId: string) => Promise<{
    model: string | null;
    local: boolean;
    reasoning_profile: "none" | "toggle" | "levels" | null;
  }>;
}

/** The outcome of an action on presets: done, or the reason shown. */
export type PresetOutcome =
  | { ok: true }
  | { ok: false; reason: PresetFileError | "no_model" | "refused"; detail?: string };

export interface SessionSettingsController {
  readonly sessionId: string | null;
  readonly loading: boolean;
  /** The settings in force, as the last accepted write left them. */
  readonly settings: SessionSettings;
  /** The model of the conversation, `null` when none is configured. */
  readonly model: SessionModel | null;
  /** The reasoning setting the model offers. */
  readonly reasoning: "none" | "toggle" | "levels" | null;
  /** The presets of the model of the conversation. */
  readonly presets: SessionPreset[];
  /** The preset the conversation chose. */
  readonly preset: SessionPreset | null;
  /** The settings changed over the preset. */
  readonly modified: PresetField[];
  /** Why the last write was refused. */
  readonly error: string | null;
  readonly notes: string;
  /** Whether the notes shown are the notes stored. */
  readonly notesSaved: boolean;
  load(sessionId: string): Promise<void>;
  set<K extends keyof SessionSettings>(key: K, value: SessionSettings[K] | undefined): Promise<void>;
  choose(id: number | null): Promise<void>;
  discard(): Promise<void>;
  revert(field: PresetField): Promise<void>;
  saveAsPreset(name: string): Promise<PresetOutcome>;
  newPreset(name: string): Promise<PresetOutcome>;
  importPreset(text: string): Promise<PresetOutcome>;
  /** Presets read again, after one was created elsewhere. */
  refreshPresets(): Promise<void>;
  typeNotes(text: string): void;
  flushNotes(): Promise<void>;
  dismissError(): void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * The controller of one Settings tab. `notesDelayMs` is how long after the
 * last key the notes are stored.
 */
export function createSessionSettings(
  effects: SessionSettingsEffects,
  notesDelayMs = 600,
): SessionSettingsController {
  let sessionId = $state<string | null>(null);
  let loading = $state(false);
  let settings = $state.raw<SessionSettings>({});
  let model = $state<SessionModel | null>(null);
  let reasoning = $state<"none" | "toggle" | "levels" | null>(null);
  let allPresets = $state.raw<SessionPreset[]>([]);
  let error = $state<string | null>(null);
  let notes = $state("");
  let storedNotes = $state("");
  let notesTimer: ReturnType<typeof setTimeout> | null = null;
  // Writes leave in the order the controls were touched.
  let queue: Promise<void> = Promise.resolve();

  const presets = $derived(presetsOfModel(allPresets, model));
  const preset = $derived(chosenPreset(settings, presets));
  const modified = $derived(modifiedFields(settings, preset));

  function write(next: SessionSettings): Promise<void> {
    const id = sessionId;
    if (!id) return Promise.resolve();
    const previous = settings;
    settings = next;
    error = null;
    queue = queue.then(async () => {
      try {
        const stored = await effects.setSettings(id, next);
        if (sessionId === id && settings === next) settings = stored;
      } catch (e: unknown) {
        if (sessionId === id) {
          settings = previous;
          error = messageOf(e);
        }
      }
    });
    return queue;
  }

  async function create(draft: PresetDraft): Promise<PresetOutcome> {
    try {
      const created = await effects.createPreset(draft);
      allPresets = [...allPresets.filter((p) => p.id !== created.id), created];
      await write(choosePreset(settings, created.id));
      return { ok: true };
    } catch (e: unknown) {
      return { ok: false, reason: "refused", detail: messageOf(e) };
    }
  }

  async function storeNotes(): Promise<void> {
    if (notesTimer) {
      clearTimeout(notesTimer);
      notesTimer = null;
    }
    const id = sessionId;
    const text = notes;
    if (!id || text === storedNotes) return;
    try {
      await effects.setNotes(id, text);
      if (sessionId === id) storedNotes = text;
    } catch (e: unknown) {
      if (sessionId === id) error = messageOf(e);
    }
  }

  return {
    get sessionId() { return sessionId; },
    get loading() { return loading; },
    get settings() { return settings; },
    get model() { return model; },
    get reasoning() { return reasoning; },
    get presets() { return presets; },
    get preset() { return preset; },
    get modified() { return modified; },
    get error() { return error; },
    get notes() { return notes; },
    get notesSaved() { return notes === storedNotes; },

    async load(id: string): Promise<void> {
      await storeNotes();
      sessionId = id;
      loading = true;
      error = null;
      settings = {};
      notes = "";
      storedNotes = "";
      model = null;
      reasoning = null;
      const [read, text, list, caps] = await Promise.allSettled([
        effects.getSettings(id),
        effects.getNotes(id),
        effects.listPresets(),
        effects.capabilities(id),
      ]);
      if (sessionId !== id) return;
      if (read.status === "fulfilled") settings = read.value ?? {};
      else error = messageOf(read.reason);
      if (text.status === "fulfilled") {
        notes = text.value ?? "";
        storedNotes = notes;
      }
      if (list.status === "fulfilled") allPresets = list.value ?? [];
      if (caps.status === "fulfilled" && caps.value) {
        model = sessionModelOf(caps.value);
        reasoning = caps.value.reasoning_profile;
      }
      loading = false;
    },

    set(key, value) {
      return write(withSetting(settings, key, value));
    },
    choose(id) {
      return write(choosePreset(settings, id));
    },
    discard() {
      return write(discardModifications(settings));
    },
    revert(field) {
      return write(revertField(settings, field));
    },

    async saveAsPreset(name) {
      if (!model) return { ok: false, reason: "no_model" };
      return create(presetDraftFrom(settings, preset, name, model));
    },
    async newPreset(name) {
      if (!model) return { ok: false, reason: "no_model" };
      return create(emptyPresetDraft(name, model));
    },
    async importPreset(text) {
      if (!model) return { ok: false, reason: "no_model" };
      const parsed = parsePresetFile(text, model);
      if (!parsed.ok) return { ok: false, reason: parsed.error };
      return create(parsed.draft);
    },
    async refreshPresets() {
      try {
        allPresets = await effects.listPresets();
      } catch {
        // The list shown stays; the next opening of the tab reads it again.
      }
    },

    typeNotes(text) {
      notes = text;
      if (notesTimer) clearTimeout(notesTimer);
      notesTimer = setTimeout(() => void storeNotes(), notesDelayMs);
    },
    flushNotes: storeNotes,
    dismissError() {
      error = null;
    },
  };
}
