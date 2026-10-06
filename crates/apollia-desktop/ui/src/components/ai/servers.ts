/**
 * The servers of the AI screen: what the section shows and what each gesture
 * does, kept out of the components so every gesture is held by a test that
 * drives the same object with a simulated IPC.
 *
 * A server is an address that lists its models, with a key when it asks for
 * one. Its models are read from the server each time; nothing here stores
 * them beyond the last list a server gave, which is kept so a server that
 * stops answering still shows what it offered.
 */
import { writable, type Readable } from "svelte/store";
import type {
  DeclaredServer,
  DefaultModel,
  ServerChanges,
  ServerModel,
  ServerModels,
} from "$lib/ipc/llmServers";
import {
  isHttpAddress,
  serverModelRoute,
  serverNameFor,
  testFailureOf,
  type ServerFailure,
} from "../onboarding/serverSetup";

/** Bounds of the silence a server may be allowed, in seconds. */
export const SERVER_TIMEOUT_MIN_SECS = 60;
export const SERVER_TIMEOUT_MAX_SECS = 3600;

/** Request fields the extra parameters of a server may not carry. */
export const RESERVED_EXTRA_PARAMS = [
  "model",
  "messages",
  "stream",
  "stream_options",
  "tools",
  "tool_choice",
] as const;

/** What the form of a server holds, as typed. */
export interface ServerFormState {
  name: string;
  address: string;
  /** Empty when no key is typed: no key for a new server, the saved one kept otherwise. */
  apiKey: string;
  /** Developer mode: seconds, empty for the default. */
  timeoutSecs: string;
  /** Developer mode: a JSON object, empty for none. */
  extraJson: string;
}

/** The fields of the form that can be refused, each with the reason. */
export interface ServerFormErrors {
  name?: "required" | "invalid_name" | "name_taken";
  address?: "required" | "invalid_address";
  timeoutSecs?: "invalid_timeout";
  extraJson?: "invalid_json" | "invalid_json_object" | "reserved_field";
}

const NAME_RE = /^[a-z0-9_-]+$/;

/** The form of a new server, or of `server` when one is being changed. */
export function seedServerForm(server: DeclaredServer | null): ServerFormState {
  if (server === null) {
    return { name: "", address: "", apiKey: "", timeoutSecs: "", extraJson: "" };
  }
  return {
    name: server.name,
    address: server.base_url,
    apiKey: "",
    timeoutSecs: server.timeout_secs === null ? "" : String(server.timeout_secs),
    extraJson:
      server.extra_params && Object.keys(server.extra_params).length > 0
        ? JSON.stringify(server.extra_params, null, 2)
        : "",
  };
}

/** The name a new server takes while its name field was left alone. */
export function suggestedServerName(address: string): string {
  return isHttpAddress(address) ? serverNameFor(address) : "";
}

/** The timeout the form asks for: `null` for the default, `undefined` when unreadable. */
export function parsedTimeout(value: string): number | null | undefined {
  const trimmed = value.trim();
  if (trimmed === "") return null;
  if (!/^\d+$/.test(trimmed)) return undefined;
  const secs = Number(trimmed);
  return secs >= SERVER_TIMEOUT_MIN_SECS && secs <= SERVER_TIMEOUT_MAX_SECS ? secs : undefined;
}

type ParsedExtra =
  | { ok: true; value: Record<string, unknown> | null }
  | { ok: false; error: NonNullable<ServerFormErrors["extraJson"]> };

/** The extra parameters the form asks for: `null` for none. */
export function parsedExtraParams(value: string): ParsedExtra {
  const trimmed = value.trim();
  if (trimmed === "") return { ok: true, value: null };
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return { ok: false, error: "invalid_json" };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, error: "invalid_json_object" };
  }
  const fields = parsed as Record<string, unknown>;
  if (RESERVED_EXTRA_PARAMS.some((reserved) => Object.hasOwn(fields, reserved))) {
    return { ok: false, error: "reserved_field" };
  }
  return { ok: true, value: Object.keys(fields).length === 0 ? null : fields };
}

/**
 * What the form refuses. The key is never among it: a server that asks for
 * none is saved without one, whichever server it is.
 *
 * `takenNames` are the names of the other saved servers; `developer` says
 * whether the timeout and the extra parameters are shown, and so read.
 */
export function validateServerForm(
  form: ServerFormState,
  options: { editing: boolean; developer: boolean; takenNames: readonly string[] },
): ServerFormErrors {
  const errors: ServerFormErrors = {};
  if (!options.editing) {
    const name = form.name.trim();
    if (name === "") errors.name = "required";
    else if (!NAME_RE.test(name)) errors.name = "invalid_name";
    else if (options.takenNames.includes(name)) errors.name = "name_taken";
  }
  if (form.address.trim() === "") errors.address = "required";
  else if (!isHttpAddress(form.address)) errors.address = "invalid_address";
  if (options.developer) {
    if (parsedTimeout(form.timeoutSecs) === undefined) errors.timeoutSecs = "invalid_timeout";
    const extra = parsedExtraParams(form.extraJson);
    if (!extra.ok) errors.extraJson = extra.error;
  }
  return errors;
}

/** The key as the runtime takes it: nothing when the field is empty. */
function keyOf(apiKey: string): string | null {
  const trimmed = apiKey.trim();
  return trimmed === "" ? null : trimmed;
}

function sameAddress(a: string, b: string): boolean {
  const bare = (value: string) => value.trim().replace(/\/+$/, "");
  return bare(a) === bare(b);
}

function sameJson(a: unknown, b: unknown): boolean {
  return JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
}

/**
 * What a valid form changes on `server`: only the fields that moved. An
 * empty key field keeps the saved key. The timeout and the extra parameters
 * are read only in developer mode, so a form that does not show them never
 * empties them.
 */
export function serverChanges(
  server: DeclaredServer,
  form: ServerFormState,
  developer: boolean,
): ServerChanges {
  const changes: ServerChanges = {};
  if (!sameAddress(form.address, server.base_url)) changes.base_url = form.address.trim();
  const key = keyOf(form.apiKey);
  if (key !== null) changes.api_key = key;
  if (developer) {
    const timeout = parsedTimeout(form.timeoutSecs);
    if (timeout !== undefined && timeout !== server.timeout_secs) changes.timeout_secs = timeout;
    const extra = parsedExtraParams(form.extraJson);
    if (extra.ok && !sameJson(extra.value, server.extra_params)) changes.extra_params = extra.value;
  }
  return changes;
}

/** What a test of the form found. */
export type ServerTestOutcome =
  | { phase: "listed"; models: string[] }
  | { phase: "failed"; failure: ServerFailure };

/** Where the models of one saved server stand. `models` is the last list it gave. */
export type ServerModelsState =
  | { phase: "loading"; models: ServerModel[]; expectedWindow: number | null }
  | { phase: "listed"; models: ServerModel[]; expectedWindow: number | null }
  | {
      phase: "failed";
      models: ServerModel[];
      expectedWindow: number | null;
      failure: ServerFailure;
    };

/** What the head of a server says about it. */
export type ServerStatus = "active" | "disabled" | "checking" | "unreachable" | "failing";

/** The status of `server`, from its switch and from the last answer of its models. */
export function serverStatus(
  server: DeclaredServer,
  models: ServerModelsState | undefined,
): ServerStatus {
  if (!server.enabled) return "disabled";
  if (models === undefined || models.phase === "loading") return "checking";
  if (models.phase === "listed") return "active";
  return models.failure.kind === "unreachable" ? "unreachable" : "failing";
}

/** The catalogue key of each status. Written out so an unread key is found by its absence. */
export const SERVER_STATUS_LABEL: Record<ServerStatus, string> = {
  active: "ai.servers.status.active",
  disabled: "ai.servers.status.disabled",
  checking: "ai.servers.status.checking",
  unreachable: "ai.servers.status.unreachable",
  failing: "ai.servers.status.failing",
};

/** The catalogue key of each refusal of the form. */
export const SERVER_FORM_ERROR_TEXT: Record<
  NonNullable<ServerFormErrors[keyof ServerFormErrors]>,
  string
> = {
  required: "ai.servers.form.error.required",
  invalid_name: "ai.servers.form.error.invalid_name",
  name_taken: "ai.servers.form.error.name_taken",
  invalid_address: "ai.servers.form.error.invalid_address",
  invalid_timeout: "dev.ai.servers.error.invalid_timeout",
  invalid_json: "dev.ai.servers.error.invalid_json",
  invalid_json_object: "dev.ai.servers.error.invalid_json_object",
  reserved_field: "dev.ai.servers.error.reserved_field",
};

/** Whether `model` of `server` is the default model of the machine. */
export function isDefaultModel(
  defaultModel: DefaultModel | null,
  server: string,
  model: string,
): boolean {
  if (defaultModel === null || defaultModel.model_source !== "server") return false;
  const route = serverModelRoute(server, model);
  return defaultModel.model_id === route || defaultModel.backend === route;
}

export interface ServersState {
  /** False until the saved servers were read once. */
  loaded: boolean;
  /** Why the saved servers could not be read, `null` when they were. */
  error: string | null;
  servers: DeclaredServer[];
  /** The models of each server, by server name. */
  models: Record<string, ServerModelsState>;
  defaultModel: DefaultModel | null;
  /** The server a gesture is in flight on, `null` otherwise. */
  busy: string | null;
}

/** Everything the section asks of the application, so a test can stand in for it. */
export interface ServersDeps {
  list: () => Promise<DeclaredServer[]>;
  models: (name: string) => Promise<ServerModels>;
  test: (baseUrl: string, apiKey: string | null) => Promise<string[]>;
  save: (name: string, baseUrl: string, apiKey: string | null) => Promise<DeclaredServer>;
  update: (name: string, changes: ServerChanges) => Promise<DeclaredServer>;
  remove: (name: string) => Promise<void>;
  getDefault: () => Promise<DefaultModel | null>;
  setDefault: (modelSource: "server", modelId: string) => Promise<DefaultModel>;
  /** Makes the engines of this application read the new default. */
  reloadRouter: () => Promise<void>;
}

export interface Servers extends Readable<ServersState> {
  /** Reads the saved servers, the default model, and the models of every enabled server. */
  refresh: () => Promise<void>;
  /** Asks the saved server `name` for its models: the test of a saved server, and its retry. */
  loadModels: (name: string) => Promise<void>;
  /**
   * Tests what the form holds, saving nothing. A saved server whose address
   * and key were left alone is reached with the key it was saved with.
   */
  testForm: (form: ServerFormState, editing: DeclaredServer | null) => Promise<ServerTestOutcome>;
  /** Saves a new server, or the changes of `editing`. Rejects with the refusal of the runtime. */
  saveForm: (
    form: ServerFormState,
    editing: DeclaredServer | null,
    developer: boolean,
  ) => Promise<DeclaredServer>;
  /** Turns the server on or off. Off, its models are no longer offered. */
  setEnabled: (name: string, enabled: boolean) => Promise<void>;
  /** Makes `model` of `server` the default model of the machine. */
  setDefault: (server: string, model: string) => Promise<void>;
  /** Forgets the server and its key. */
  remove: (name: string) => Promise<void>;
}

const INITIAL: ServersState = {
  loaded: false,
  error: null,
  servers: [],
  models: {},
  defaultModel: null,
  busy: null,
};

function sentenceOf(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  if (err !== null && typeof err === "object") {
    const message = (err as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(err);
}

export function createServers(deps: ServersDeps): Servers {
  const store = writable<ServersState>(INITIAL);
  let state = INITIAL;

  function publish(patch: Partial<ServersState>): void {
    state = { ...state, ...patch };
    store.set(state);
  }

  function publishModels(name: string, models: ServerModelsState): void {
    publish({ models: { ...state.models, [name]: models } });
  }

  async function loadModels(name: string): Promise<void> {
    const known = state.models[name];
    const last = known?.models ?? [];
    const expectedWindow = known?.expectedWindow ?? null;
    publishModels(name, { phase: "loading", models: last, expectedWindow });
    try {
      const answer = await deps.models(name);
      publishModels(name, {
        phase: "listed",
        models: answer.models,
        expectedWindow: answer.expected_context_window,
      });
    } catch (err: unknown) {
      publishModels(name, {
        phase: "failed",
        models: last,
        expectedWindow,
        failure: testFailureOf(err),
      });
    }
  }

  async function readServers(): Promise<DeclaredServer[] | null> {
    try {
      const servers = await deps.list();
      const kept: Record<string, ServerModelsState> = {};
      for (const server of servers) {
        const known = state.models[server.name];
        if (known) kept[server.name] = known;
      }
      publish({ loaded: true, error: null, servers, models: kept });
      return servers;
    } catch (err: unknown) {
      publish({ error: sentenceOf(err) });
      return null;
    }
  }

  async function readDefault(): Promise<void> {
    try {
      publish({ defaultModel: await deps.getDefault() });
    } catch {
      // The default is a mark on a row: without it the rows still list.
    }
  }

  async function refresh(): Promise<void> {
    const [servers] = await Promise.all([readServers(), readDefault()]);
    if (servers === null) return;
    await Promise.all(servers.filter((s) => s.enabled).map((s) => loadModels(s.name)));
  }

  async function testForm(
    form: ServerFormState,
    editing: DeclaredServer | null,
  ): Promise<ServerTestOutcome> {
    if (!isHttpAddress(form.address)) {
      return { phase: "failed", failure: { kind: "invalid_address", detail: null } };
    }
    const key = keyOf(form.apiKey);
    try {
      const untouched =
        editing !== null && key === null && sameAddress(form.address, editing.base_url);
      const models = untouched
        ? (await deps.models(editing.name)).models.map((model) => model.id)
        : await deps.test(form.address.trim(), key);
      if (models.length === 0) {
        return { phase: "failed", failure: { kind: "no_models", detail: null } };
      }
      return { phase: "listed", models };
    } catch (err: unknown) {
      return { phase: "failed", failure: testFailureOf(err) };
    }
  }

  async function saveForm(
    form: ServerFormState,
    editing: DeclaredServer | null,
    developer: boolean,
  ): Promise<DeclaredServer> {
    let saved: DeclaredServer;
    if (editing === null) {
      const name = form.name.trim();
      saved = await deps.save(name, form.address.trim(), keyOf(form.apiKey));
      const settings = serverChanges(saved, { ...form, apiKey: "" }, developer);
      delete settings.base_url;
      if (Object.keys(settings).length > 0) saved = await deps.update(name, settings);
    } else {
      const changes = serverChanges(editing, form, developer);
      saved = Object.keys(changes).length > 0 ? await deps.update(editing.name, changes) : editing;
    }
    await readServers();
    if (saved.enabled) await loadModels(saved.name);
    return saved;
  }

  async function during(name: string, gesture: () => Promise<void>): Promise<void> {
    if (state.busy !== null) return;
    publish({ busy: name });
    try {
      await gesture();
    } finally {
      publish({ busy: null });
    }
  }

  function setEnabled(name: string, enabled: boolean): Promise<void> {
    return during(name, async () => {
      await deps.update(name, { enabled });
      await readServers();
      if (enabled) await loadModels(name);
    });
  }

  function setDefault(server: string, model: string): Promise<void> {
    return during(server, async () => {
      const defaultModel = await deps.setDefault("server", serverModelRoute(server, model));
      await deps.reloadRouter();
      publish({ defaultModel });
    });
  }

  function remove(name: string): Promise<void> {
    return during(name, async () => {
      await deps.remove(name);
      await Promise.all([readServers(), readDefault()]);
    });
  }

  return {
    subscribe: store.subscribe,
    refresh,
    loadModels,
    testForm,
    saveForm,
    setEnabled,
    setDefault,
    remove,
  };
}

/** A number of tokens as the locale writes it, with its thousands separator. */
export function tokenCount(tokens: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(tokens);
}

/**
 * What the content shows: that no server is connected yet, a saved server,
 * the form of a new one, or the change of one.
 */
export type ServersView =
  | { kind: "none" }
  | { kind: "server"; name: string }
  | { kind: "add" }
  | { kind: "edit"; name: string };
