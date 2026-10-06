/**
 * Declaring an inference server inside the AI step of the first launch: what
 * the form shows and what each gesture does.
 *
 * The decisions live here, outside the component, for two reasons. The person
 * may go back a step and return: the server chosen is still chosen, so the
 * setup object outlives the component. And every outcome (listed, refused,
 * chosen, save failed) is then held by a test that drives the same object
 * with a simulated IPC.
 */
import { writable, type Readable } from "svelte/store";

/** Where the form stands. */
export type ServerSetupPhase =
  /** Nothing tested, or the address changed since the last test. */
  | "idle"
  /** The server is being asked for its models. */
  | "testing"
  /** The server answered with its models. */
  | "listed"
  /** The test did not list any model. */
  | "failed";

/** The kinds of failure the form knows how to explain. */
export const SERVER_FAILURE_KINDS = [
  "invalid_address",
  "unreachable",
  "key_refused",
  "not_openai_compatible",
  "policy",
  "no_models",
  "save",
  "other",
] as const;

export type ServerFailureKind = (typeof SERVER_FAILURE_KINDS)[number];

/**
 * The catalogue keys of each kind of failure: what happened, and what to
 * check. Written out in full so a key nothing reads is found by its absence.
 */
export const SERVER_FAILURE_TEXT: Record<ServerFailureKind, { title: string; body: string }> = {
  invalid_address: {
    title: "onboarding.ai_setup.server.error.invalid_address_title",
    body: "onboarding.ai_setup.server.error.invalid_address_body",
  },
  unreachable: {
    title: "onboarding.ai_setup.server.error.unreachable_title",
    body: "onboarding.ai_setup.server.error.unreachable_body",
  },
  key_refused: {
    title: "onboarding.ai_setup.server.error.key_refused_title",
    body: "onboarding.ai_setup.server.error.key_refused_body",
  },
  not_openai_compatible: {
    title: "onboarding.ai_setup.server.error.not_openai_compatible_title",
    body: "onboarding.ai_setup.server.error.not_openai_compatible_body",
  },
  policy: {
    title: "onboarding.ai_setup.server.error.policy_title",
    body: "onboarding.ai_setup.server.error.policy_body",
  },
  no_models: {
    title: "onboarding.ai_setup.server.error.no_models_title",
    body: "onboarding.ai_setup.server.error.no_models_body",
  },
  save: {
    title: "onboarding.ai_setup.server.error.save_title",
    body: "onboarding.ai_setup.server.error.save_body",
  },
  other: {
    title: "onboarding.ai_setup.server.error.other_title",
    body: "onboarding.ai_setup.server.error.other_body",
  },
};

/** An address of the shape the form expects, shown as its example. */
export const SERVER_ADDRESS_EXAMPLE = "http://localhost:11434";

/** A failure the form shows; `detail` is the runtime's own sentence. */
export interface ServerFailure {
  kind: ServerFailureKind;
  detail: string | null;
}

/** The server model that became the default model. */
export interface ChosenServerModel {
  /** Name the server was saved under. */
  server: string;
  /** Identifier the server gives the model. */
  model: string;
  /** Address the server was saved with. */
  baseUrl: string;
}

export interface ServerSetupState {
  phase: ServerSetupPhase;
  /** The models of the last successful test, in the order of the server. */
  models: string[];
  failure: ServerFailure | null;
  /** The model being saved as the default, `null` otherwise. */
  choosing: string | null;
  chosen: ChosenServerModel | null;
}

/** Everything the setup asks of the application, so a test can stand in for it. */
export interface ServerSetupDeps {
  test: (baseUrl: string, apiKey: string | null) => Promise<string[]>;
  save: (name: string, baseUrl: string, apiKey: string | null) => Promise<unknown>;
  setDefault: (modelSource: "server", modelId: string) => Promise<unknown>;
}

export interface ServerSetup extends Readable<ServerSetupState> {
  /** Asks the server at `baseUrl` for its models. Saves nothing. */
  test: (baseUrl: string, apiKey: string) => Promise<void>;
  /** The address or the key changed: what was listed no longer describes it. */
  edited: () => void;
  /** Saves the tested server and makes `model` the default model. */
  choose: (model: string) => Promise<void>;
}

const INITIAL: ServerSetupState = {
  phase: "idle",
  models: [],
  failure: null,
  choosing: null,
  chosen: null,
};

/** Whether `baseUrl` is an `http://` or `https://` address with a host. */
export function isHttpAddress(baseUrl: string): boolean {
  return /^https?:\/\/[^/?#\s]+/i.test(baseUrl.trim());
}

/**
 * The name a server is saved under, read from its address: the host and the
 * port, in the characters a server name accepts. The same address always
 * gives the same name, so testing and choosing again replaces the server
 * instead of piling up copies.
 */
export function serverNameFor(baseUrl: string): string {
  const authority = baseUrl
    .trim()
    .toLowerCase()
    .replace(/^https?:\/\//, "")
    .split(/[/?#]/)[0]
    .replace(/^.*@/, "");
  const name = authority.replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  return name === "" ? "server" : name;
}

/** The name a model of a saved server is addressed by. */
export function serverModelRoute(server: string, model: string): string {
  return `${server}/${model}`;
}

function sentenceOf(err: unknown): string | null {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (err !== null && typeof err === "object") {
    const message = (err as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return null;
}

/** What a rejected test says, in the kinds the form explains. */
export function testFailureOf(err: unknown): ServerFailure {
  const kind =
    err !== null && typeof err === "object" ? (err as { kind?: unknown }).kind : undefined;
  const known = SERVER_FAILURE_KINDS.find((candidate) => candidate === kind);
  return { kind: known ?? "other", detail: sentenceOf(err) };
}

/** The value the runtime takes for a key field: nothing when it is empty. */
function keyOf(apiKey: string): string | null {
  const trimmed = apiKey.trim();
  return trimmed === "" ? null : trimmed;
}

export function createServerSetup(deps: ServerSetupDeps): ServerSetup {
  const store = writable<ServerSetupState>(INITIAL);
  let state = INITIAL;
  /** What the last successful test was run with: the server `choose` saves. */
  let tested: { baseUrl: string; apiKey: string | null } | null = null;
  /** Counts the tests and the edits, so a late answer is told from the current one. */
  let generation = 0;

  function publish(patch: Partial<ServerSetupState>): void {
    state = { ...state, ...patch };
    store.set(state);
  }

  async function test(baseUrl: string, apiKey: string): Promise<void> {
    const address = baseUrl.trim();
    const mine = ++generation;
    tested = null;
    if (!isHttpAddress(address)) {
      publish({ phase: "failed", models: [], failure: { kind: "invalid_address", detail: null } });
      return;
    }
    publish({ phase: "testing", models: [], failure: null });
    const key = keyOf(apiKey);
    try {
      const models = await deps.test(address, key);
      if (mine !== generation) return;
      if (models.length === 0) {
        publish({ phase: "failed", failure: { kind: "no_models", detail: null } });
        return;
      }
      tested = { baseUrl: address, apiKey: key };
      publish({ phase: "listed", models });
    } catch (err: unknown) {
      if (mine !== generation) return;
      publish({ phase: "failed", failure: testFailureOf(err) });
    }
  }

  function edited(): void {
    if (state.phase === "idle" && state.failure === null) return;
    generation += 1;
    tested = null;
    publish({ phase: "idle", models: [], failure: null });
  }

  async function choose(model: string): Promise<void> {
    const server = tested;
    if (server === null || state.choosing !== null || !state.models.includes(model)) return;
    const name = serverNameFor(server.baseUrl);
    publish({ choosing: model, failure: null });
    try {
      await deps.save(name, server.baseUrl, server.apiKey);
      await deps.setDefault("server", serverModelRoute(name, model));
      publish({ choosing: null, chosen: { server: name, model, baseUrl: server.baseUrl } });
    } catch (err: unknown) {
      publish({ choosing: null, failure: { kind: "save", detail: sentenceOf(err) } });
    }
  }

  return { subscribe: store.subscribe, test, edited, choose };
}
