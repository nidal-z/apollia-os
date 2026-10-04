import { describe, test, expect, vi, beforeAll, beforeEach } from "vitest";
import { render as renderToString } from "svelte/server";
import type { Component } from "svelte";
import { waitLocale } from "svelte-i18n";
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
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(async () => false),
  requestPermission: vi.fn(async () => "denied"),
  sendNotification: vi.fn(),
}));

import ChatMessageScroller from "./ChatMessageScroller.svelte";
import ChatPlanHost from "./ChatPlanHost.svelte";
import ChatMessageBubble from "./ChatMessageBubble.svelte";
import PlanModeHost from "$lib/components/operator/PlanModeHost.svelte";
import { developerMode } from "$lib/stores/developerMode";
import { chatPlanState } from "$lib/stores/chatPlanMode";
import { planModeState } from "$lib/stores/planMode";
import { liveApprovalCall } from "./liveApprovalCall";
import type { ChatMessageView } from "$lib/types";
import type { A2ADelegation } from "./useA2ADelegation.svelte";
import type { ConversationScroll } from "./useConversationScroll.svelte";

/**
 * Vitest runs in its node environment, where `svelte` resolves to the server
 * build: the markup is what these tests assert on, and the server render walks
 * the same template branches as the browser.
 */
function html<P extends Record<string, unknown>>(component: Component<P>, props: P): string {
  return renderToString(component as Component<Record<string, unknown>>, { props }).body;
}

function hasTestid(markup: string, prefix: string): boolean {
  return new RegExp(`data-testid="${prefix}`).test(markup);
}

const idleA2A = {
  active: null,
  steps: [],
  guardMessage: null,
  elapsed: 0,
  running: false,
  tick: () => {},
} as unknown as A2ADelegation;

const idleScroll = {
  container: undefined,
  released: false,
  showJump: false,
  unread: 0,
  toBottom: () => {},
  onScroll: () => {},
  jumpToLatest: () => {},
} as unknown as ConversationScroll;

function scrollerWithLiveApproval(): string {
  return html(ChatMessageScroller, {
    sessionId: "s-1",
    sessionMode: "libre" as const,
    sessionAgentName: null,
    sessionStatus: "processing" as const,
    messageGroups: [],
    empty: false,
    isStreaming: false,
    isProcessing: false,
    tokenBuffer: "",
    liveToolChain: [],
    liveSkin: "operator" as const,
    hasCrossSessionRefs: false,
    a2a: idleA2A,
    scroll: idleScroll,
    pendingApproval: {
      sessionId: "s-1",
      messageId: "m-1",
      toolCallId: "call-1",
      toolName: "file_write",
      inputPreview: `Tool 'file_write' asks to run with: {"path":"notes.md","content":"hi"}`,
    },
    pendingUserInput: null,
    onregenerate: () => {},
    onedit: () => {},
  });
}

function assistantMessage(overrides: Partial<ChatMessageView>): ChatMessageView {
  return {
    id: "m-1",
    role: "assistant",
    content: "",
    tool_calls: null,
    tool_name: null,
    seq: 1,
    created_at: "2026-10-04T10:00:00Z",
    metadata: null,
    ...overrides,
  };
}

beforeAll(async () => {
  await waitLocale();
});

beforeEach(() => {
  developerMode.set(false);
});

describe("live approval card follows the developer mode", () => {
  test("the default card is mounted when the developer mode is off", () => {
    // GIVEN the developer mode off and a live approval request
    developerMode.set(false);

    // WHEN the conversation body renders
    const markup = scrollerWithLiveApproval();

    // THEN the default card shows, and the raw card does not
    expect(hasTestid(markup, "operator-approval-file_write")).toBe(true);
    expect(hasTestid(markup, "approval-card-")).toBe(false);
  });

  test("the raw card is mounted when the developer mode is on", () => {
    // GIVEN the developer mode on and the same live approval request
    developerMode.set(true);

    // WHEN the conversation body renders
    const markup = scrollerWithLiveApproval();

    // THEN the raw card shows, and the default card does not
    expect(hasTestid(markup, "approval-card-file_write")).toBe(true);
    expect(hasTestid(markup, "operator-approval-")).toBe(false);
  });
});

describe("liveApprovalCall", () => {
  test("recovers the arguments from the runtime prompt", () => {
    // GIVEN the prompt a ChatApprovalRequired event carries
    const prompt = `Tool 'file_read' asks to run with: {"path":"a.txt"}`;

    // WHEN the pending call is rebuilt
    const call = liveApprovalCall("file_read", prompt);

    // THEN the arguments parse out of the prompt
    expect(call).toEqual({
      tool_name: "file_read",
      input: { path: "a.txt" },
      output: null,
      status: "pending",
    });
  });

  test("a truncated or free-text prompt leaves the arguments empty", () => {
    // GIVEN a prompt whose JSON was truncated, and one with none
    const truncated = `Tool 'file_read' asks to run with: {"path":"a.t…`;

    // WHEN the pending calls are rebuilt
    const fromTruncated = liveApprovalCall("file_read", truncated);
    const fromText = liveApprovalCall("bash_executor", "Allow bash execution?");

    // THEN no argument is invented
    expect(fromTruncated.input).toEqual({});
    expect(fromText.input).toEqual({});
  });
});

describe("chat plan review follows the developer mode", () => {
  function pendingChatPlan(): void {
    chatPlanState.set({
      status: "pending_approval",
      sessionId: "s-1",
      phase: "awaiting_approval",
      plan: { plan_id: "p-1", revision: 1, status: "draft", steps: [] },
      lastMutation: null,
      errorMessage: null,
    });
  }

  test("developer mode off shows the compact review", () => {
    // GIVEN a session plan awaiting approval, developer mode off
    pendingChatPlan();
    developerMode.set(false);

    // WHEN the host renders
    const markup = html(ChatPlanHost, { sessionId: "s-1" });

    // THEN the compact review shows, not the detailed one
    expect(hasTestid(markup, "chat-plan-review\"")).toBe(true);
    expect(hasTestid(markup, "chat-plan-review-builder")).toBe(false);
  });

  test("developer mode on shows the detailed review", () => {
    // GIVEN the same plan, developer mode on
    pendingChatPlan();
    developerMode.set(true);

    // WHEN the host renders
    const markup = html(ChatPlanHost, { sessionId: "s-1" });

    // THEN the detailed review shows
    expect(hasTestid(markup, "chat-plan-review-builder")).toBe(true);
    expect(hasTestid(markup, "chat-plan-review\"")).toBe(false);
  });
});

describe("run plan review follows the developer mode", () => {
  function pendingRunPlan(): void {
    planModeState.set({
      status: "pending_approval",
      runId: "r-1",
      plan: {
        run_id: "r-1",
        plan_id: "p-1",
        task_id: "t-1",
        step_count: 1,
        steps: [
          { step_id: "s1", description: "Read the file", depends_on: [], tool_hint: null, model_hint: null },
        ],
        ttl_secs: 600,
      } as never,
      previousPlan: null,
      errorMessage: null,
    });
  }

  test("developer mode off shows the compact review", () => {
    // GIVEN a run plan awaiting approval, developer mode off
    pendingRunPlan();
    developerMode.set(false);

    // WHEN the global host renders
    const markup = html(PlanModeHost, {});

    // THEN the compact review shows, not the detailed one
    expect(hasTestid(markup, "plan-review\"")).toBe(true);
    expect(hasTestid(markup, "plan-review-builder")).toBe(false);
  });

  test("developer mode on shows the detailed review", () => {
    // GIVEN the same plan, developer mode on
    pendingRunPlan();
    developerMode.set(true);

    // WHEN the global host renders
    const markup = html(PlanModeHost, {});

    // THEN the detailed review shows
    expect(hasTestid(markup, "plan-review-builder")).toBe(true);
    expect(hasTestid(markup, "plan-review\"")).toBe(false);
  });
});

describe("persisted approval and empty answer follow the developer mode", () => {
  const pending = assistantMessage({
    content: "",
    tool_calls: [{ tool_name: "file_write", input: { path: "a.md" }, output: null, status: "pending" }],
  });

  test("developer mode off mounts the default card", () => {
    // GIVEN a stored turn with a pending call, developer mode off
    developerMode.set(false);

    // WHEN the message renders
    const markup = html(ChatMessageBubble, { message: pending, sessionId: "s-1" });

    // THEN the default card shows, not the raw one
    expect(hasTestid(markup, "operator-approval-file_write")).toBe(true);
    expect(hasTestid(markup, "approval-card-")).toBe(false);
  });

  test("developer mode on mounts the raw card", () => {
    // GIVEN the same turn, developer mode on
    developerMode.set(true);

    // WHEN the message renders
    const markup = html(ChatMessageBubble, { message: pending, sessionId: "s-1" });

    // THEN the raw card shows, not the default one
    expect(hasTestid(markup, "approval-card-file_write")).toBe(true);
    expect(hasTestid(markup, "operator-approval-")).toBe(false);
  });

  test("an empty answer takes the default wording off, the developer wording on", async () => {
    // GIVEN a finished turn with no text
    const { get } = await import("svelte/store");
    const { t } = await import("svelte-i18n");
    const empty = assistantMessage({ content: "" });
    const plain = get(t)("chat.empty_response_operator");
    const detailed = get(t)("chat.empty_response_builder");

    // WHEN it renders in both modes
    developerMode.set(false);
    const off = html(ChatMessageBubble, { message: empty, sessionId: "s-1" });
    developerMode.set(true);
    const on = html(ChatMessageBubble, { message: empty, sessionId: "s-1" });

    // THEN each mode carries its own wording
    const escape = (s: string) => s.replace(/'/g, "&#39;");
    expect(off.includes(plain) || off.includes(escape(plain))).toBe(true);
    expect(off.includes(detailed) || off.includes(escape(detailed))).toBe(false);
    expect(on.includes(detailed) || on.includes(escape(detailed))).toBe(true);
  });
});
