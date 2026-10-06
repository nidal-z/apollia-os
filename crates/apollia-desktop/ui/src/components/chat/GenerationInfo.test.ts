import { describe, test, expect, vi, beforeAll, beforeEach } from "vitest";
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

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
// The node environment has no DOM for the sanitizer; these tests read text only.
vi.mock("dompurify", () => ({
  default: { isSupported: false, sanitize: (html: string) => html, addHook: () => {} },
}));

// The tooltip of the application needs the provider mounted at its root and
// shows nothing until hovered. Here it is replaced by a plain wrapper that
// writes its sentence next to what it explains, so a test can read both.
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

import GenerationInfo from "./GenerationInfo.svelte";
import ChatMessageBubble from "./ChatMessageBubble.svelte";
import { generationInfoItems, readGeneration } from "$lib/chat/generationStats";
import { loadTurnCalls, resetTurnCalls, turnCallsOf } from "$lib/chat/turnCalls.svelte";
import type { ChatMessageView } from "$lib/types";

import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const TOTAL = {
  calls: 2,
  output_tokens: 412,
  tokens_per_second: 38.2,
  ttft_ms: 480,
  duration_ms: 11600,
  stop_reason: "completed",
};

const CALLS = [
  {
    backend: "local",
    model: "qwen3-8b",
    prompt_tokens: 3120,
    completion_tokens: 212,
    ttft_ms: 480,
    latency_ms: 5200,
    generation_ms: 4700,
    finish_reason: "tool_calls",
  },
  {
    backend: "local",
    model: "qwen3-8b",
    prompt_tokens: 3940,
    completion_tokens: 200,
    ttft_ms: null,
    latency_ms: 5600,
    generation_ms: null,
    finish_reason: "stop",
  },
];

function answer(metadata: Record<string, unknown> | null): ChatMessageView {
  return {
    id: "answer-1",
    role: "assistant",
    content: "The March invoices add up to 18,420.",
    tool_calls: null,
    tool_name: null,
    seq: 2,
    created_at: "2026-10-04T10:42:00Z",
    metadata,
  };
}

function line(generation: Record<string, unknown>, initiallyExpanded = false): string {
  return render(GenerationInfo, {
    props: {
      generation: readGeneration({ generation })!,
      sessionId: "s1",
      turnId: "answer-1",
      stepCount: 2,
      initiallyExpanded,
    },
  }).body;
}

function bubble(message: ChatMessageView): string {
  return render(ChatMessageBubble, { props: { message, sessionId: "s1" } }).body;
}

function lookup(catalogue: unknown, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node !== null && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      catalogue,
    );
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale("en");
});

beforeEach(() => {
  resetTurnCalls();
  invoke.mockReset();
});

describe("the generation line under an answer", () => {
  test("it shows speed, tokens, first token, duration and stop reason", () => {
    // GIVEN the complete total of a turn
    // WHEN the line renders
    const html = line(TOTAL);

    // THEN the five figures are there, each on its own anchor
    expect(html).toContain("38.2 tokens/s");
    expect(html).toContain("412 tokens");
    expect(html).toContain("First token 0.48 s");
    expect(html).toContain("Took 11.6 s");
    expect(html).toContain("Finished normally");
    for (const id of ["speed", "tokens", "ttft", "duration", "stop"]) {
      expect(html).toContain(`data-testid="chat-generation-${id}-answer-1"`);
    }
  });

  test("an unknown figure reads not measured, never zero", () => {
    // GIVEN a total whose engine reported no first token and no speed
    // WHEN the line renders
    const html = line({ calls: 1, output_tokens: 90, duration_ms: 2000, stop_reason: "length" });

    // THEN both say so and no zero is invented
    expect(html).toContain("First token not measured");
    expect(html).toContain("Speed not measured");
    expect(html).not.toContain("First token 0");
    expect(html).toContain("Length limit reached");
  });

  test("the answer of a reloaded conversation shows its line, another shows none", () => {
    // GIVEN an answer read back from the store with its total, and one without
    const reloaded = JSON.parse(JSON.stringify({ generation: TOTAL })) as Record<string, unknown>;

    // WHEN both bubbles render
    const withTotal = bubble(answer(reloaded));
    const without = bubble(answer({ thinking_trace: "…" }));

    // THEN only the first carries the line
    expect(withTotal).toContain('data-testid="chat-generation-answer-1"');
    expect(withTotal).toContain("412 tokens");
    expect(without).not.toContain("chat-generation-");
  });

  test("a message of the user never carries the line", () => {
    // GIVEN a user message whose metadata happens to hold a total
    const message: ChatMessageView = { ...answer({ generation: TOTAL }), role: "user" };

    // WHEN it renders
    const html = bubble(message);

    // THEN there is no line
    expect(html).not.toContain("chat-generation-");
  });
});

describe("the explanation of each figure", () => {
  test("every figure, measured or not, has one sentence in both languages", () => {
    // GIVEN the lines of a complete total and of one with nothing measured
    const items = [
      ...generationInfoItems(readGeneration({ generation: TOTAL })!, "fr"),
      ...generationInfoItems(
        readGeneration({ generation: { calls: 1, output_tokens: 0, stop_reason: "error" } })!,
        "fr",
      ),
    ];

    // WHEN the explanation and the text of each are looked up in the catalogues
    // THEN each exists, and the explanation is one sentence
    for (const item of items) {
      for (const catalogue of [fr, en]) {
        const help = lookup(catalogue, item.helpKey);
        expect(typeof lookup(catalogue, item.labelKey), item.labelKey).toBe("string");
        expect(typeof help, item.helpKey).toBe("string");
        expect((help as string).trim().endsWith(".")).toBe(true);
        expect((help as string).split(". ").length, item.helpKey).toBe(1);
      }
    }
  });

  test("each figure of the line is wrapped in its own explanation", () => {
    // GIVEN the complete total of a turn
    // WHEN the line renders
    const html = line(TOTAL);

    // THEN each of the five figures sits inside the tooltip that explains it
    for (const id of ["speed", "tokens", "ttft", "duration", "stop"]) {
      const help = lookup(en, `chat.generation.help.${id}`) as string;
      const wrapped = new RegExp(
        `data-tooltip="${help.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}">(?:<!--[^>]*-->|\\s)*<span[^>]*data-testid="chat-generation-${id}-answer-1"`,
      );
      expect(html, id).toMatch(wrapped);
    }
  });

  test("the first token is explained as the delay before the first word", () => {
    // GIVEN the catalogue
    // WHEN the explanation of the first token is read
    const help = lookup(en, "chat.generation.help.ttft") as string;

    // THEN it says what is measured and which call counts
    expect(help).toContain("the delay between sending and the first word received");
    expect(help).toContain("that of the first");
  });
});

describe("the detail per call", () => {
  test("folded, the line offers the detail and reads nothing", () => {
    // GIVEN a line that was never unfolded
    // WHEN it renders
    const html = line(TOTAL);

    // THEN the toggle is there, the detail is not, and no call was read
    expect(html).toContain('data-testid="chat-generation-detail-toggle-answer-1"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain("chat-generation-detail-answer-1");
    expect(invoke).not.toHaveBeenCalled();
  });

  test("unfolded, it lists each call with its model, tokens, first token, duration and end", async () => {
    // GIVEN a turn whose two calls were read through the command
    invoke.mockResolvedValueOnce(CALLS);
    await loadTurnCalls("s1", "answer-1");

    // WHEN the line renders unfolded
    const html = line(TOTAL, true);

    // THEN the command was asked for that turn, and both calls are listed
    expect(invoke).toHaveBeenCalledWith("list_turn_llm_calls", {
      sessionId: "s1",
      turnId: "answer-1",
    });
    expect(html).toContain("2 steps · 2 model calls · 11.6 s");
    expect(html).toContain("Model call 1");
    expect(html).toContain("Model call 2");
    expect(html).toContain("qwen3-8b");
    expect(html).toContain("3,120");
    expect(html).toContain("tool requested");
    expect(html).toContain("finished normally");
    expect(html).toMatch(/chat-generation-call-ttft-answer-1-1"[^>]*>\s*not measured/);
    expect(html).toMatch(/chat-generation-call-duration-answer-1-0"[^>]*>\s*5.2 s/);
  });

  test("a turn read once is not read again, a failed read is", async () => {
    // GIVEN a first read that fails, then one that answers
    invoke.mockRejectedValueOnce(new Error("runtime unreachable"));
    invoke.mockResolvedValueOnce(CALLS);

    // WHEN the turn is read three times
    await loadTurnCalls("s1", "answer-1");
    const failed = turnCallsOf("s1", "answer-1");
    await loadTurnCalls("s1", "answer-1");
    await loadTurnCalls("s1", "answer-1");

    // THEN the failure was shown, retried once, and the result kept
    expect(failed).toEqual({ status: "error", message: "runtime unreachable" });
    expect(turnCallsOf("s1", "answer-1")).toEqual({ status: "ready", calls: CALLS });
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  test("a failed read says so under the answer", async () => {
    // GIVEN a turn whose calls could not be read
    invoke.mockRejectedValueOnce(new Error("runtime unreachable"));
    await loadTurnCalls("s1", "answer-1");

    // WHEN the line renders unfolded
    const html = line(TOTAL, true);

    // THEN the reason is shown instead of an empty list
    expect(html).toContain("runtime unreachable");
    expect(html).toContain('data-testid="chat-generation-error-answer-1"');
  });
});
