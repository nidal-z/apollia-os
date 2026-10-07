/**
 * The prompt template the model of a conversation receives, as the Settings
 * tab shows it.
 *
 * The runtime answers where the text comes from: the preset the conversation
 * chose, the engine serving the model, or nowhere it can be read. The answer
 * depends on the conversation, its model, its preset and whether the model
 * is loaded, so it is read again whenever one of them changes.
 */
import type { LoadedModelsResponse } from "$lib/types";

/** Where the template comes from. */
export type PromptTemplateSource = "preset" | "engine" | "unavailable";

/** Answer of the runtime for one conversation. */
export interface PromptTemplateAnswer {
  source: PromptTemplateSource;
  /** The template, `null` when it cannot be read. */
  template: string | null;
}

/** The answer while nothing can be shown. */
export const PROMPT_TEMPLATE_UNAVAILABLE: PromptTemplateAnswer = {
  source: "unavailable",
  template: null,
};

/** What makes the template of a conversation: a change in any is a new read. */
export interface PromptTemplateSubject {
  sessionId: string | null;
  /** The model the conversation names, `null` for the default. */
  model: string | null;
  /** The preset the runtime holds for the conversation. */
  preset: number | undefined;
  /** The resident models, `null` until read once. */
  loaded: LoadedModelsResponse | null;
  /**
   * How many messages the conversation holds: its first answer is what loads
   * a model nobody loaded by hand.
   */
  messages: number;
}

/**
 * The key of a subject: two subjects with the same key read the same
 * template, `null` when there is no conversation to read.
 */
export function promptTemplateKey(subject: PromptTemplateSubject): string | null {
  if (!subject.sessionId) return null;
  const resident = (subject.loaded?.models ?? [])
    .map((m) => m.modelPath)
    .sort()
    .join(",");
  return [
    subject.sessionId,
    subject.model ?? "",
    subject.preset ?? "",
    resident,
    subject.messages,
  ].join("|");
}

/** A usable answer, or "unavailable" for anything else the runtime sends. */
export function promptTemplateAnswer(raw: unknown): PromptTemplateAnswer {
  if (!raw || typeof raw !== "object") return PROMPT_TEMPLATE_UNAVAILABLE;
  const { source, template } = raw as { source?: unknown; template?: unknown };
  const known = source === "preset" || source === "engine";
  if (known && typeof template === "string" && template.trim() !== "") {
    return { source, template };
  }
  return PROMPT_TEMPLATE_UNAVAILABLE;
}

export interface PromptTemplateReader {
  /** The answer shown; "unavailable" until the first read comes back. */
  readonly answer: PromptTemplateAnswer;
  readonly loading: boolean;
  /** The conversation the answer shown belongs to. */
  readonly sessionId: string | null;
  /** Reads the template of `sessionId`; the last call wins. */
  refresh(sessionId: string): Promise<void>;
  /** Forgets the answer, when no conversation is shown. */
  clear(): void;
}

/** The reader of one Settings tab, over the runtime call `read`. */
export function createPromptTemplate(
  read: (sessionId: string) => Promise<unknown>,
): PromptTemplateReader {
  let answer = $state.raw<PromptTemplateAnswer>(PROMPT_TEMPLATE_UNAVAILABLE);
  let loading = $state(false);
  let sessionId = $state<string | null>(null);
  let turn = 0;

  return {
    get answer() { return answer; },
    get loading() { return loading; },
    get sessionId() { return sessionId; },

    async refresh(id: string): Promise<void> {
      const mine = ++turn;
      // The template of another conversation is never shown under this one.
      if (sessionId !== id) answer = PROMPT_TEMPLATE_UNAVAILABLE;
      sessionId = id;
      loading = true;
      let next = PROMPT_TEMPLATE_UNAVAILABLE;
      try {
        next = promptTemplateAnswer(await read(id));
      } catch {
        // A read that fails shows what a model that is not loaded shows.
      }
      if (mine !== turn) return;
      answer = next;
      loading = false;
    },

    clear(): void {
      turn += 1;
      sessionId = null;
      answer = PROMPT_TEMPLATE_UNAVAILABLE;
      loading = false;
    },
  };
}
