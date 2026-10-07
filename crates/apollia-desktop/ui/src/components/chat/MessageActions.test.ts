import { describe, test, expect, vi, beforeAll } from "vitest";
import { render as renderToString } from "svelte/server";
import type { Component } from "svelte";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
// The node environment has no DOM for the sanitizer; these tests read markup
// around the answer, not the answer.
vi.mock("dompurify", () => ({
  default: { isSupported: false, sanitize: (html: string) => html, addHook: () => {} },
}));

import ChatMessageBubble from "./ChatMessageBubble.svelte";
import MessageGroup from "./MessageGroup.svelte";
import { ANSWER_ACTIONS, type AnswerActionsContext } from "$lib/chat/answerActions";
import type { ChatMessageView } from "$lib/types";

/**
 * Vitest runs in its node environment, where `svelte` resolves to the server
 * build: the markup is what these tests assert on, and the server render walks
 * the same template branches as the browser.
 */
function html<P extends Record<string, unknown>>(
  component: Component<P>,
  props: P,
  answers?: AnswerActionsContext,
): string {
  return renderToString(component as Component<Record<string, unknown>>, {
    props,
    context: new Map(answers ? [[ANSWER_ACTIONS, answers]] : []),
  }).body;
}

function message(id: string, role: ChatMessageView["role"], extra: Partial<ChatMessageView> = {}): ChatMessageView {
  return {
    id,
    role,
    content: `text of ${id}`,
    tool_calls: null,
    tool_name: null,
    seq: 0,
    created_at: "2026-10-06T10:00:00Z",
    ...extra,
  };
}

function conversation(overrides: Partial<AnswerActionsContext> = {}): AnswerActionsContext {
  return {
    lastAnswerId: "a-2",
    continueRefusal: null,
    continueAnswer: () => {},
    fork: () => {},
    editAnswer: () => {},
    remove: () => {},
    ...overrides,
  };
}

/** The actions drawn under `id`, by the name in their test id, in order. */
function actions(markup: string, id: string): string[] {
  const row = markup.slice(markup.indexOf(`data-testid="chat-message-actions-${id}"`));
  return Array.from(row.matchAll(new RegExp(`data-testid="chat-message-([a-z]+)-${id}"`, "g")), (m) => m[1]!)
    .filter((name) => name !== "actions");
}

function bubble(m: ChatMessageView, answers?: AnswerActionsContext): string {
  return html(
    ChatMessageBubble,
    { message: m, sessionId: "s-1", onregenerate: () => {}, onedit: () => {} },
    answers,
  );
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

describe("the actions under an answer", () => {
  test("the last answer offers the six actions in their order", () => {
    // GIVEN a conversation whose model continues an answer
    // WHEN its last answer is drawn
    const markup = bubble(message("a-2", "assistant"), conversation());

    // THEN regenerate, continue, branch off, copy, edit and delete follow
    // one another, and continue is not disabled
    expect(actions(markup, "a-2")).toEqual(["regenerate", "continue", "fork", "copy", "edit", "delete"]);
    expect(markup).toMatch(/aria-disabled="false"[^>]*data-testid="chat-message-continue-a-2"/);
    expect(markup).toContain('title="Continue"');
  });

  test("an earlier answer offers every action but continue", () => {
    // GIVEN the same conversation
    // WHEN an answer that is not the last is drawn
    const markup = bubble(message("a-1", "assistant"), conversation());

    // THEN continue is absent and the order of the others holds
    expect(actions(markup, "a-1")).toEqual(["regenerate", "fork", "copy", "edit", "delete"]);
  });

  test("on a model that does not continue an answer, continue is disabled with its reason", () => {
    // GIVEN a conversation whose model does not continue an answer
    const refused = conversation({ continueRefusal: "chat.continue_unavailable_engine" });

    // WHEN its last answer is drawn
    const markup = bubble(message("a-2", "assistant"), refused);

    // THEN the button is there, disabled, and says why
    expect(actions(markup, "a-2")).toContain("continue");
    expect(markup).toMatch(/aria-disabled="true"[^>]*data-testid="chat-message-continue-a-2"/);
    expect(markup).toContain("Unavailable: this model cannot pick an answer back up.");
  });

  test("a user message offers edit and delete", () => {
    // GIVEN the same conversation
    // WHEN a user message is drawn
    const markup = bubble(message("u-1", "user"), conversation());

    // THEN it can be edited, which sends it again, and deleted
    expect(actions(markup, "u-1")).toEqual(["edit", "delete"]);
  });

  test("outside a conversation, regenerate and copy keep their place and nothing else is drawn", () => {
    // GIVEN no conversation around the bubble
    // WHEN an answer, a user message and an error line are drawn
    const answer = bubble(message("a-1", "assistant"));
    const user = bubble(message("u-1", "user"));
    const error = bubble(message("error-1", "system"), conversation());

    // THEN the answer keeps regenerate and copy, the user message its edit,
    // and a line the runtime does not hold offers none of the new actions
    expect(actions(answer, "a-1")).toEqual(["regenerate", "copy"]);
    expect(actions(user, "u-1")).toEqual(["edit"]);
    expect(actions(error, "error-1")).toEqual(["regenerate", "copy"]);
  });

  test("an edited answer is marked beside the name of its agent", () => {
    // GIVEN a group of one answer the runtime marked as edited, and one it
    // did not
    const group = (m: ChatMessageView) => ({
      key: m.id,
      role: "assistant" as const,
      messages: [m],
      startedAt: m.created_at,
    });
    const edited = message("a-1", "assistant", { metadata: { edited: true } });

    // WHEN each group is drawn
    const marked = html(MessageGroup, { group: group(edited), sessionId: "s-1" });
    const plain = html(MessageGroup, { group: group(message("a-2", "assistant")), sessionId: "s-1" });

    // THEN only the edited one carries the mark
    expect(marked).toContain('data-testid="chat-message-edited-a-1"');
    expect(marked).toContain(">edited<");
    expect(plain).not.toContain("chat-message-edited-");
  });
});
