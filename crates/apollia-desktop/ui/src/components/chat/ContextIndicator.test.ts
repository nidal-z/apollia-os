import { readFileSync } from "node:fs";
import { resolve } from "node:path";
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

// The tooltip of the application needs the provider mounted at its root and
// shows nothing until hovered. Here it is replaced by a plain wrapper that
// writes what it says next to what it explains, so a test can read both.
type Renderer = { push: (html: string) => void };
vi.mock("$lib/components/ui/tooltip", () => ({
  Tooltip: (
    renderer: Renderer,
    props: {
      content: string | ((renderer: Renderer) => void);
      children?: (renderer: Renderer) => void;
      trigger?: (renderer: Renderer, props: Record<string, unknown>) => void;
    },
  ) => {
    if (props.trigger) props.trigger(renderer, {});
    else props.children?.(renderer);
    renderer.push('<div data-tooltip="">');
    if (typeof props.content === "string") renderer.push(props.content);
    else props.content(renderer);
    renderer.push("</div>");
  },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(async () => false),
  requestPermission: vi.fn(async () => "denied"),
  sendNotification: vi.fn(),
}));

import ContextIndicator, { gaugeRows } from "./ContextIndicator.svelte";
import {
  CONTEXT_GAUGE_HIGH_PCT,
  contextGaugeDetail,
  ringDashOffset,
} from "$lib/chat/contextGauge";
import { chatConversationStats } from "$lib/stores/chat";
import { refreshSessionMetrics } from "$lib/stores/chatMetrics";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

/** Feeds the metrics of `sessionId` as the runtime would answer them. */
async function measured(
  sessionId: string,
  context_window_tokens: number,
  context_tokens_used: number,
  exchanges_count = 1,
): Promise<void> {
  invoke.mockResolvedValueOnce({
    session_id: sessionId,
    prompt_tokens: context_tokens_used,
    completion_tokens: 0,
    cache_read_input_tokens: 0,
    cache_write_input_tokens: 0,
    cost_usd: null,
    budget_max_steps: 25,
    steps_used: 5,
    context_window_tokens,
    context_tokens_used,
    tool_stats: [],
    exchanges_count,
    started_at: null,
    updated_at: null,
    active_duration_ms: 0,
    llm_backend: null,
  });
  await refreshSessionMetrics(sessionId, true);
}

function gauge(sessionId: string): string {
  return render(ContextIndicator, { props: { sessionId } }).body;
}

/**
 * The label and the value of line `id` of the hover, `null` when the line is
 * absent. Narrow spaces of a formatted number read as plain spaces.
 */
function row(html: string, id: string): [string, string] | null {
  const start = html.indexOf(`data-testid="context-gauge-row-${id}"`);
  if (start === -1) return null;
  const texts = [...html.slice(start).matchAll(/<span[^>]*>([^<]*)<\/span>/g)]
    .slice(0, 2)
    .map((m) => m[1].replace(/\s/g, " "));
  return [texts[0] ?? "", texts[1] ?? ""];
}

function label(catalogue: typeof fr, key: string): string {
  return key
    .split(".")
    .reduce<unknown>((node, part) => (node as Record<string, unknown>)[part], catalogue) as string;
}

beforeAll(async () => {
  await waitLocale();
});

beforeEach(async () => {
  invoke.mockReset();
  await locale.set("fr");
});

describe("contextGaugeDetail - what the hover counts", () => {
  test("a measured context gives its tokens, its share and what is left", () => {
    // GIVEN a context of 8 192 tokens, 983 of them used
    // WHEN the detail is derived
    const detail = contextGaugeDetail(8192, 983);

    // THEN it carries the tokens of the conversation, the loaded context, the
    // share in whole percent and the remainder
    expect(detail).toEqual({ used: 983, window: 8192, pct: 12, remaining: 7209 });
  });

  test("an overfull report clamps the share and never leaves a negative rest", () => {
    // GIVEN a report of more tokens than the context holds
    // WHEN the detail is derived
    const detail = contextGaugeDetail(100, 250);

    // THEN the share stops at 100 and nothing is left
    expect(detail?.pct).toBe(100);
    expect(detail?.remaining).toBe(0);
  });

  test("an unmeasured context has no detail", () => {
    // GIVEN a backend that reports no context window
    // WHEN the detail is derived
    // THEN there is none: zero is an absent measurement, not an empty context
    expect(contextGaugeDetail(0, 500)).toBeNull();
  });

  test("the ring is empty at 0, half drawn at 50 and closed at 100", () => {
    // GIVEN a ring of circumference 40
    // WHEN its dash offset is derived for three shares and two out of range
    // THEN the offset goes from the whole circumference down to zero
    expect(ringDashOffset(0, 40)).toBe(40);
    expect(ringDashOffset(50, 40)).toBe(20);
    expect(ringDashOffset(100, 40)).toBe(0);
    expect(ringDashOffset(140, 40)).toBe(0);
    expect(ringDashOffset(-5, 40)).toBe(40);
  });
});

describe("gaugeRows - the lines of the hover", () => {
  const detail = contextGaugeDetail(8192, 983);

  test("the hover says the three counts, in the words of the interface", () => {
    // GIVEN a measured context whose prompt does not carry the profile
    // WHEN the lines of the hover are derived
    const rows = gaugeRows(detail, false);

    // THEN there are three, in order, each with its tokens
    expect(rows.map((r) => r.id)).toEqual(["conversation_tokens", "loaded_context", "share_used"]);
    expect(rows.map((r) => r.tokens)).toEqual([983, 8192, 7209]);
    // AND each has its words in the French interface, and reads in English as
    expect(rows.map((r) => label(fr, r.labelKey))).toEqual([
      fr.chat.gauge.conversation_tokens,
      fr.chat.gauge.loaded_context,
      fr.chat.gauge.share_used,
    ]);
    expect(rows.map((r) => label(en, r.labelKey))).toEqual([
      "Conversation tokens",
      "Loaded context",
      "Share used (left)",
    ]);
  });

  test("« Profil inclus » is the fourth line while the profile is in the prompt", () => {
    // GIVEN the same context, its prompt carrying the profile
    // WHEN the lines of the hover are derived
    const rows = gaugeRows(detail, true);

    // THEN a fourth line states it, without a count
    expect(rows).toHaveLength(4);
    expect(rows[3].id).toBe("profile_included");
    expect(rows[3].tokens).toBeNull();
    expect(label(fr, rows[3].labelKey)).toBe(fr.chat.gauge.profile_included);
    expect(label(en, rows[3].labelKey)).toBe("Profile included");
  });

  test("the step budget is no line of the hover", () => {
    // GIVEN every line the hover can show
    // WHEN their identifiers and labels are read
    const rows = gaugeRows(detail, true);
    const words = rows.map((r) => `${r.id} ${label(fr, r.labelKey)} ${label(en, r.labelKey)}`);

    // THEN none is about steps or a budget
    expect(words.join(" ")).not.toMatch(/budget|step|\u00e9tape/i);
  });

  test("an unmeasured context keeps only the profile line", () => {
    // GIVEN no measured context
    // WHEN the lines are derived with and without the profile
    // THEN no count is invented
    expect(gaugeRows(null, false)).toEqual([]);
    expect(gaugeRows(null, true).map((r) => r.id)).toEqual(["profile_included"]);
  });
});

describe("ContextIndicator - a ring and a percentage", () => {
  test("it draws a ring and the share of the context in use", async () => {
    // GIVEN a conversation using 983 of 8 192 tokens after one exchange
    await measured("g-low", 8192, 983);

    // WHEN the gauge renders
    const html = gauge("g-low");

    // THEN it is a ring of two circles and « 12% », not in its high state
    expect(html).toContain('data-testid="context-indicator-footer"');
    expect(html.match(/<circle/g)).toHaveLength(2);
    expect(html).toMatch(/data-testid="context-gauge-percent"[^>]*>12%</);
    expect(html).toContain('data-high="false"');
    expect(html).toContain(`aria-label="${fr.chat.gauge.aria.replace("{value}", "12%")}"`);
  });

  test("it no longer prints the step budget", async () => {
    // GIVEN a conversation that used 5 of its 25 steps
    await measured("g-budget", 8192, 983);

    // WHEN the gauge renders, in both languages
    const french = gauge("g-budget");
    await locale.set("en");
    const english = gauge("g-budget");

    // THEN neither mentions a budget, nor the 20 % of steps used
    for (const html of [french, english]) {
      expect(html).not.toMatch(/budget/i);
      expect(html).not.toMatch(/\b20\s?%/);
    }
  });

  test("its hover details the context, and the profile once it is in the prompt", async () => {
    // GIVEN a conversation using 983 of 8 192 tokens, its prompt without the
    // profile, then with it
    await measured("g-hover", 8192, 983);
    const stats = (injected: boolean) => ({
      message_count: 2,
      summarized_count: 0,
      context_usage_pct: 12,
      user_memory_injected: injected,
      cross_sessions_referenced: 0,
    });

    // WHEN the gauge renders in each state
    chatConversationStats.set(stats(false));
    const without = gauge("g-hover");
    chatConversationStats.set(stats(true));
    const withProfile = gauge("g-hover");
    chatConversationStats.set(null);

    // THEN the hover reads the three counts, with the share and what is left
    expect(row(without, "conversation_tokens")).toEqual([fr.chat.gauge.conversation_tokens, "983"]);
    expect(row(without, "loaded_context")).toEqual([fr.chat.gauge.loaded_context, "8 192"]);
    expect(row(without, "share_used")).toEqual([fr.chat.gauge.share_used, "12 % (7 209)"]);
    // AND the profile line only while the profile is part of the prompt
    expect(row(without, "profile_included")).toBeNull();
    expect(row(withProfile, "profile_included")?.[0]).toBe(fr.chat.gauge.profile_included);
  });

  test("it warns from the high share on", async () => {
    // GIVEN a conversation at 91 % of its context
    await measured("g-high", 1000, 910);

    // WHEN the gauge renders
    const html = gauge("g-high");

    // THEN it is in its high state
    expect(CONTEXT_GAUGE_HIGH_PCT).toBe(90);
    expect(html).toMatch(/data-testid="context-gauge-percent"[^>]*>91%</);
    expect(html).toContain('data-high="true"');
  });

  test("an unmeasured context reads --, never a share", async () => {
    // GIVEN an engine that reports no context window
    await measured("g-unknown", 0, 0);

    // WHEN the gauge renders
    const html = gauge("g-unknown");

    // THEN it shows « -- »
    expect(html).toMatch(/data-testid="context-gauge-percent"[^>]*>--</);
  });

  test("it stays out of a conversation with no exchange yet", async () => {
    // GIVEN a conversation where nothing was exchanged
    await measured("g-new", 8192, 0, 0);

    // WHEN the gauge renders
    // THEN nothing is drawn
    expect(gauge("g-new")).not.toContain("context-indicator-footer");
  });
});

describe("the composer holds the only context gauge", () => {
  const read = (file: string) => readFileSync(resolve(__dirname, file), "utf8");

  test("the gauge sits in the toolbar of the composer, right of the chips", () => {
    // GIVEN the toolbar of the composer
    const toolbar = read("ChatComposerToolbar.svelte");

    // WHEN the order of its controls is read
    const chips = toolbar.indexOf("<ComposerCapabilityChips");
    const spacer = toolbar.indexOf('<div class="flex-1"></div>');
    const indicator = toolbar.indexOf("<ContextIndicator");
    const send = toolbar.indexOf('data-testid="chat-send-button"');

    // THEN the chips come first, then the spacer, the gauge and the send button
    expect(chips).toBeGreaterThan(-1);
    expect(chips).toBeLessThan(spacer);
    expect(spacer).toBeLessThan(indicator);
    expect(indicator).toBeLessThan(send);
  });

  test("no other component of the chat mounts a gauge", () => {
    // GIVEN the conversation and its configuration panel
    // WHEN their sources are read
    // THEN neither mounts the gauge nor draws a context bar
    for (const file of ["ChatConversation.svelte", "ChatConfigPanelBody.svelte"]) {
      const source = read(file);
      expect(source).not.toContain("<ContextIndicator");
      expect(source).not.toContain("context-window-bar");
    }
  });
});
