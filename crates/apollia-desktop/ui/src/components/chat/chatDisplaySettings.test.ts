import { describe, test, expect, vi, beforeAll, afterEach } from "vitest";
import { render } from "svelte/server";
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
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
// The node environment has no DOM for the sanitizer; these tests read text only.
vi.mock("dompurify", () => ({
  default: { isSupported: false, sanitize: (html: string) => html, addHook: () => {} },
}));
// The tooltip needs its provider at the root; a plain wrapper is enough here.
vi.mock("$lib/components/ui/tooltip", () => ({
  Tooltip: (
    renderer: { push: (html: string) => void },
    props: { content: string; children: (renderer: unknown) => void },
  ) => {
    renderer.push(`<span data-tooltip="${props.content}">`);
    props.children(renderer);
    renderer.push("</span>");
  },
}));

import MessageRenderer, { answerRendering } from "./MessageRenderer.svelte";
import ChatMessageBubble, { userMessageFrame } from "./ChatMessageBubble.svelte";
import MessageGroup, { groupShowsHeader } from "./MessageGroup.svelte";
import ReasoningBlock, { showsVignette } from "./ReasoningBlock.svelte";
import AppearanceChatSettings from "../settings/AppearanceChatSettings.svelte";
import { lastAnswerId, showsGenerationInfo } from "$lib/chat/generationStats";
import {
  CHAT_APPEARANCE_DEFAULTS,
  chatAppearance,
  setChatAppearance,
} from "$lib/stores/chatAppearance";
import type { ChatMessageView } from "$lib/types";

import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const TOTAL = {
  calls: 1,
  output_tokens: 412,
  tokens_per_second: 38.2,
  ttft_ms: 480,
  duration_ms: 11600,
  stop_reason: "completed",
};

function message(
  id: string,
  role: ChatMessageView["role"],
  content: string,
  metadata: Record<string, unknown> | null = null,
): ChatMessageView {
  return {
    id,
    role,
    content,
    tool_calls: null,
    tool_name: null,
    seq: 1,
    created_at: "2026-10-04T10:42:00Z",
    metadata,
  };
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale("en");
});

afterEach(() => {
  chatAppearance.set({ ...CHAT_APPEARANCE_DEFAULTS });
});

describe("view mode - Markdown or plain text", () => {
  const ANSWER = "## Total\n\nThe **March** invoices add up to 18,420.";

  test("by default an answer is rendered", () => {
    // GIVEN the default view mode
    // WHEN an answer written in Markdown renders
    const html = render(MessageRenderer, { props: { content: ANSWER } }).body;

    // THEN its marks became markup
    expect(html).toContain("<strong>March</strong>");
    expect(html).not.toContain("**March**");
    expect(html).not.toContain('data-testid="message-plain"');
  });

  test("in plain text an answer is shown as received", () => {
    // GIVEN the view mode set to plain text
    setChatAppearance({ viewMode: "plain" });

    // WHEN the same answer renders
    const html = render(MessageRenderer, { props: { content: ANSWER } }).body;

    // THEN the text is there mark for mark, and nothing was rendered
    expect(html).toContain('data-testid="message-plain"');
    expect(html).toContain("## Total");
    expect(html).toContain("**March**");
    expect(html).not.toContain("<strong>");
    expect(html).not.toContain("<h2");
  });

  test("plain text also leaves confidence markers unparsed", () => {
    // GIVEN an answer that carries markers, which have their own rendering
    // WHEN the rendering is resolved in each mode
    const rendered = answerRendering({ viewMode: "markdown", hasMarkup: true });
    const plain = answerRendering({ viewMode: "plain", hasMarkup: true });

    // THEN the markers are rendered in the first, left as text in the second
    expect(rendered).toBe("assertions");
    expect(plain).toBe("plain");
  });
});

describe("generation info - last message only, or all messages", () => {
  const thread = [
    message("q1", "user", "First question"),
    message("a1", "assistant", "First answer", { generation: TOTAL }),
    message("q2", "user", "Second question"),
    message("a2", "assistant", "Second answer", { generation: TOTAL }),
    message("q3", "user", "A question still waiting for its answer"),
  ];

  function bubbles(): { first: string; last: string } {
    const lastId = lastAnswerId(thread);
    const one = (m: ChatMessageView): string =>
      render(ChatMessageBubble, { props: { message: m, sessionId: "s1", lastAnswerId: lastId } })
        .body;
    return { first: one(thread[1]), last: one(thread[3]) };
  }

  test("the last answer is the last message that carries figures", () => {
    // GIVEN a thread ending on a question not answered yet
    // WHEN the last answer is looked up
    const id = lastAnswerId(thread);

    // THEN it is the second answer, so its line stays while the next arrives
    expect(id).toBe("a2");
    expect(lastAnswerId([thread[0]])).toBeNull();
  });

  test("by default only the last answer shows its line", () => {
    // GIVEN the default, "last message only"
    // WHEN both answers render
    const { first, last } = bubbles();

    // THEN the earlier answer has no line and the last one has it
    expect(first).not.toContain("chat-generation-");
    expect(last).toContain('data-testid="chat-generation-a2"');
    expect(last).toContain("412 tokens");
  });

  test("with all messages, every answer shows its line", () => {
    // GIVEN the setting on "all messages"
    setChatAppearance({ generationInfo: "all" });

    // WHEN both answers render
    const { first, last } = bubbles();

    // THEN each carries its own line
    expect(first).toContain('data-testid="chat-generation-a1"');
    expect(last).toContain('data-testid="chat-generation-a2"');
  });

  test("the rule itself, on its two values", () => {
    // GIVEN an earlier answer of a thread whose last answer is another
    const earlier = { messageId: "a1", lastAnswerId: "a2" };

    // WHEN the rule is asked under each value
    const underLast = showsGenerationInfo({ scope: "last", ...earlier });
    const underAll = showsGenerationInfo({ scope: "all", ...earlier });

    // THEN it is hidden under the first and shown under the second
    expect(underLast).toBe(false);
    expect(underAll).toBe(true);
  });

  test("the setting explains itself behind its question mark, in both languages", () => {
    // GIVEN the "Chat style" block
    // WHEN it renders
    const html = render(AppearanceChatSettings).body;

    // THEN the row carries a help anchor holding the sentence, which exists in
    // French and in English and differs between them
    expect(html).toContain('data-testid="appearance-generation-info-help"');
    expect(html).toContain(en.settings.appearance.generation_info_help);
    expect(fr.settings.appearance.generation_info_help).not.toBe(
      en.settings.appearance.generation_info_help,
    );
  });
});

describe("message style - bubbles or blocks", () => {
  const sent = message("q1", "user", "How much did we invoice in March?");

  test("by default a user message is a bubble on the right", () => {
    // GIVEN the default style
    // WHEN a user message renders
    const html = render(ChatMessageBubble, { props: { message: sent, sessionId: "s1" } }).body;

    // THEN it sits in the bubble, aligned to the end of the column
    expect(html).toContain("chat-user-bubble");
    expect(html).toContain("items-end");
    expect(html).toContain('data-message-style="bubble"');
    expect(html).not.toContain("chat-user-block");
  });

  test("in blocks, a user message takes the width of the column with no bubble", () => {
    // GIVEN the style set to blocks
    setChatAppearance({ messageStyle: "block" });

    // WHEN the same message renders
    const html = render(ChatMessageBubble, { props: { message: sent, sessionId: "s1" } }).body;

    // THEN the bubble is gone and the message is a full-width block
    expect(html).not.toContain("chat-user-bubble");
    expect(html).toContain("chat-user-block w-full");
    expect(html).toContain('data-message-style="block"');
    expect(html).not.toContain("items-end");
  });

  test("a message being edited takes the column in either style", () => {
    // GIVEN a message being edited
    // WHEN its frame is resolved in each style
    const bubble = userMessageFrame({ style: "bubble", editing: true });
    const block = userMessageFrame({ style: "block", editing: true });

    // THEN neither keeps the bubble
    expect(bubble.frame).toBe("w-full");
    expect(block.frame).toContain("w-full");
    expect(block.frame).not.toContain("chat-user-bubble");
  });

  test("in blocks, a user turn is named, since no bubble tells it from the answer", () => {
    // GIVEN a group of one user message
    const group = { role: "user" as const, key: "q1", messages: [sent], startedAt: sent.created_at };

    // WHEN it renders in bubbles, then in blocks
    const inBubbles = render(MessageGroup, { props: { group, sessionId: "s1" } }).body;
    setChatAppearance({ messageStyle: "block" });
    const inBlocks = render(MessageGroup, { props: { group, sessionId: "s1" } }).body;

    // THEN only the block carries the name row; an answer always does
    expect(inBubbles).not.toContain(">You<");
    expect(inBlocks).toContain(">You<");
    expect(groupShowsHeader("assistant", "bubble")).toBe(true);
    expect(groupShowsHeader("system", "block")).toBe(false);
  });
});

describe("reasoning - the vignette of the block", () => {
  test("it is on by default, beside unfold by default", () => {
    // GIVEN the "Reasoning" block of Appearance at its defaults
    // WHEN it renders
    const html = render(AppearanceChatSettings).body;

    // THEN both switches are there, the vignette after the unfold one
    const unfold = html.indexOf('data-testid="appearance-reasoning-expand-toggle"');
    const vignette = html.indexOf('data-testid="appearance-reasoning-vignette-toggle"');
    expect(unfold).toBeGreaterThan(-1);
    expect(vignette).toBeGreaterThan(unfold);
    expect(CHAT_APPEARANCE_DEFAULTS.reasoningVignette).toBe(true);
    expect(CHAT_APPEARANCE_DEFAULTS.expandReasoning).toBe(false);
  });

  test("it fades a thought still arriving once its text passes the block", () => {
    // GIVEN the setting on
    // WHEN a live thought is short, then clipped, then finished
    const short = showsVignette({ enabled: true, live: true, clipped: false });
    const clipped = showsVignette({ enabled: true, live: true, clipped: true });
    const finished = showsVignette({ enabled: true, live: false, clipped: false });

    // THEN only the clipped live thought fades
    expect(short).toBe(false);
    expect(clipped).toBe(true);
    expect(finished).toBe(false);
  });

  test("off, nothing fades", () => {
    // GIVEN the setting off and a clipped live thought
    setChatAppearance({ reasoningVignette: false });

    // WHEN the rule is asked and the block renders
    const shown = showsVignette({ enabled: false, live: true, clipped: true });
    const html = render(ReasoningBlock, {
      props: { content: "Reading the workbook.", live: true, expanded: true, ontoggle: () => {} },
    }).body;

    // THEN there is no fade
    expect(shown).toBe(false);
    expect(html).not.toContain('data-testid="reasoning-vignette"');
  });
});

describe("the Chat style block - defaults and labels", () => {
  test("every new control is there, in the order of the screen", () => {
    // GIVEN the block at its defaults
    // WHEN it renders
    const html = render(AppearanceChatSettings).body;

    // THEN view mode, generation info, the two scroll switches and the message
    // style come in that order, the follow switch right under the send one
    const order = [
      "appearance-view-mode-select",
      "appearance-generation-info-select",
      "appearance-scroll-sent-top-toggle",
      "appearance-follow-generation-toggle",
      "appearance-message-style-bubble",
      "appearance-message-style-block",
    ].map((id) => html.indexOf(`data-testid="${id}"`));
    expect(order.every((at) => at > -1)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
  });

  test("every new label exists in French and in English", () => {
    // GIVEN the keys the new controls read
    const keys = [
      "nav_position_label",
      "nav_position_hint",
      "nav_position_left",
      "nav_position_right",
      "view_mode_label",
      "view_mode_hint",
      "view_mode_markdown",
      "view_mode_plain",
      "generation_info_label",
      "generation_info_help",
      "generation_info_last",
      "generation_info_all",
      "scroll_sent_top_label",
      "follow_generation_label",
      "message_style_label",
      "message_style_bubble",
      "message_style_block",
      "reasoning_vignette_label",
      "reasoning_vignette_hint",
    ] as const;

    // WHEN each is read in both catalogues
    // THEN both sides carry a sentence, and they differ
    for (const key of keys) {
      const f = fr.settings.appearance[key];
      const e = en.settings.appearance[key];
      expect(f, key).toBeTruthy();
      expect(e, key).toBeTruthy();
      expect(f, key).not.toBe(e);
    }
  });
});
