import { describe, test, expect, vi, beforeAll } from "vitest";
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

import ReasoningBlock, { reasoningTitleKey } from "./ReasoningBlock.svelte";
import ReasoningCard from "./ReasoningCard.svelte";
import ChatMessageBubble from "./ChatMessageBubble.svelte";
import { resolveInitialExpanded } from "./ReasoningCard.svelte";
import { setChatAppearance } from "$lib/stores/chatAppearance";
import { developerMode } from "$lib/stores/developerMode";
import type { ChatMessageView } from "$lib/types";

const THOUGHT = "I read the attached workbook and filter the Date column on March.";

beforeAll(async () => {
  locale.set("en");
  await waitLocale("en");
});

function block(props: { durationMs?: number | null; expanded: boolean; live?: boolean }): string {
  return render(ReasoningBlock, {
    props: { content: THOUGHT, ontoggle: () => {}, ...props },
  }).body;
}

describe("reasoning block - title and fold", () => {
  test("folded, the block is one line titled by its duration, the thought hidden", () => {
    // GIVEN a reasoning fragment that took four seconds, folded
    // WHEN it renders
    const html = block({ durationMs: 4_200, expanded: false });

    // THEN the title says how long, and the thought is not in the markup
    expect(html).toContain("Thought for 4 s");
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain("filter the Date column");
  });

  test("unfolded, the same title heads the thought", () => {
    // GIVEN the same fragment, unfolded
    // WHEN it renders
    const html = block({ durationMs: 4_200, expanded: true });

    // THEN the title stays and the thought shows
    expect(html).toContain("Thought for 4 s");
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain("filter the Date column");
  });

  test("with no measured time the title says only that it thought", () => {
    // GIVEN a fragment of a conversation reopened later, with no measure
    // WHEN it renders
    const html = block({ durationMs: null, expanded: false });

    // THEN it is titled without a time
    expect(html).toContain("Thought");
    expect(html).not.toMatch(/Thought for/);
  });

  test("a fragment still streaming is titled as thinking", () => {
    // GIVEN the open fragment of a live turn
    // WHEN its title is chosen
    const title = reasoningTitleKey({ live: true, durationMs: null, kind: "thinking" });

    // THEN it says the agent is thinking, with no duration
    expect(title).toEqual({ key: "chat.reasoning.thinking_live", withDuration: false });
  });
});

describe("reasoning block - folded by default, unless the setting says otherwise", () => {
  test("a reasoning block starts folded, and unfolded with the setting on", () => {
    // GIVEN a reasoning fragment the user never touched
    const base = { stored: null, defaultExpanded: undefined, kind: "thinking" as const };

    // WHEN it opens with the setting off, then on
    const off = resolveInitialExpanded({ ...base, expandByDefault: false });
    const on = resolveInitialExpanded({ ...base, expandByDefault: true });

    // THEN it is folded, then unfolded
    expect(off).toBe(false);
    expect(on).toBe(true);
  });

  test("the setting unfolds the reasoning, never a tool step", () => {
    // GIVEN a tool step, the setting on
    const input = { stored: null, defaultExpanded: undefined, expandByDefault: true, kind: "tool_call" as const };

    // WHEN it resolves its first state
    // THEN it stays folded: the setting is about reasoning
    expect(resolveInitialExpanded(input)).toBe(false);
  });

  test("the card renders the fragment folded with the setting off, unfolded with it on", () => {
    // GIVEN a stored fragment with a measured duration
    const item = {
      id: "m-think-0",
      kind: "thinking" as const,
      status: "success" as const,
      content: THOUGHT,
      duration_ms: 4_000,
    };

    // WHEN the card renders under each value of the setting
    setChatAppearance({ expandReasoning: false });
    const folded = render(ReasoningCard, { props: { item, persist: false } }).body;
    setChatAppearance({ expandReasoning: true });
    const unfolded = render(ReasoningCard, { props: { item, persist: false } }).body;
    setChatAppearance({ expandReasoning: false });

    // THEN the thought shows only with the setting on
    expect(folded).not.toContain("filter the Date column");
    expect(unfolded).toContain("filter the Date column");
  });
});

describe("reasoning block - one block per fragment, among the steps", () => {
  test("a turn renders one block per fragment, interleaved with its steps, and no strip", () => {
    // GIVEN a stored turn: thought, search, thought
    developerMode.set(false);
    const message: ChatMessageView = {
      id: "m-1",
      role: "assistant",
      content: "The total is 18,420 EUR.",
      tool_calls: [
        { tool_name: "web_search", input: { query: "rates" }, output: null, status: "executed" },
      ],
      tool_name: null,
      seq: 1,
      created_at: "2026-10-04T10:00:00Z",
      metadata: {
        thinking_trace: "I look up the rates.\n\n---\n\nI add them up.",
        reasoning_boundaries: [0, 1],
      },
    };

    // WHEN the message renders
    const html = render(ChatMessageBubble, { props: { message, sessionId: "s-1" } }).body;

    // THEN two reasoning blocks frame the step, in that order, with no activity strip
    const first = html.indexOf('data-testid="reasoning-card-thinking"');
    const step = html.indexOf('data-testid="reasoning-card-tool_call"');
    const second = html.indexOf('data-testid="reasoning-card-thinking"', first + 1);
    expect(first).toBeGreaterThan(-1);
    expect(step).toBeGreaterThan(first);
    expect(second).toBeGreaterThan(step);
    expect(html).not.toContain("activity-strip");
  });
});
