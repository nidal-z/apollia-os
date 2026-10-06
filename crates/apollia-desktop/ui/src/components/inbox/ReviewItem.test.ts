import { describe, test, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// The theme store reads the browser storage at import; node has none.
vi.hoisted(() => {
  const map = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as Storage;
});

import "$lib/i18n";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import glossary from "$lib/i18n/glossary.json";
import ReviewItem from "./ReviewItem.svelte";
import { developerMode } from "$lib/stores/developerMode";
import type { DecideRequest, PendingDecision } from "$lib/ipc/pendingDecisions";

// The screen "To review" lists what the runtime gathers and answers each item
// with the card of the chat. Rendered through Svelte's server renderer, the
// only renderer the node environment offers.

function decision(over: Partial<PendingDecision>): PendingDecision {
  return {
    id: "consent:s-1::a-1::c-1",
    kind: "consent",
    agentId: "clerk",
    summary: "Tool 'file_write' asks to run",
    access: "file_write",
    family: "write_or_delete",
    detail: '{\n  "path": "/tmp/summary.txt"\n}',
    answers: ["once", "always", "refuse"],
    questions: null,
    allowOther: false,
    origin: { sessionId: "s-1", messageId: "a-1", toolCallId: "c-1" },
    askedAt: null,
    ...over,
  };
}

async function inEnglish(developer = false): Promise<void> {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(developer);
}

function html(item: PendingDecision, held: DecideRequest | null = null, title?: string): string {
  return renderToString(ReviewItem, {
    props: {
      item,
      held,
      conversationTitle: title ?? null,
      relTime: () => "just now",
      onanswer: () => {},
      oncancel: () => {},
      onopen: () => {},
    },
  }).body;
}

describe("To review - the screen and its name", () => {
  test("the screen carries the official name of the glossary in both languages", () => {
    // GIVEN the two catalogues and the glossary of the interface
    const official = { en: glossary.en.screens.review, fr: glossary.fr.screens.review };
    // WHEN the name of the screen and of its entry are read
    // THEN the English one is To review and each language says its official word
    expect(official.en).toBe("To review");
    expect(official.fr).not.toBe(official.en);
    expect(en.review.title).toBe(official.en);
    expect(fr.review.title).toBe(official.fr);
    expect(en.nav.inbox).toBe(official.en);
    expect(en.nav.sidebar.inbox).toBe(official.en);
    expect(fr.nav.inbox).toBe(official.fr);
    expect(fr.nav.sidebar.inbox).toBe(official.fr);
  });

  test("the screen lists the runtime's one list and keeps no history", () => {
    // GIVEN the source of the screen, and of the list it shares with the sidebar
    const source = readFileSync(join(__dirname, "../../routes/Inbox.svelte"), "utf-8");
    const shared = readFileSync(
      join(__dirname, "../../lib/stores/pendingDecisions.ts"),
      "utf-8",
    );
    // WHEN what it reads is looked for
    // THEN it reads the gathered list, answers through one call, and no history
    expect(source).toContain("refreshPendingDecisions()");
    expect(shared).toContain("listPendingDecisions()");
    expect(source).toContain("decidePendingDecision(request)");
    expect(source).not.toMatch(/History|history_title|listResolvedApprovals/);
  });
});

describe("To review - each item with its answers, on the card of the chat", () => {
  test("a consent shows the chat card with once, always for this agent and refuse", async () => {
    // GIVEN a consent of a conversation, the developer mode off
    await inEnglish();
    // WHEN its line is rendered
    const out = html(decision({}), null, "Requests of the week");
    // THEN the origin names the conversation and the card is the chat's, three answers
    expect(out).toContain("From the conversation “Requests of the week”");
    expect(out).toContain("Open the conversation");
    expect(out).toContain('data-testid="operator-approval-file_write"');
    expect(out).toContain('data-testid="operator-approval-accept-file_write"');
    expect(out).toContain('data-testid="operator-approval-always-file_write"');
    expect(out).toContain('data-testid="operator-approval-refuse-file_write"');
  });

  test("with the developer mode on, the consent shows the developer card of the chat", async () => {
    // GIVEN the same consent, the developer mode on
    await inEnglish(true);
    // WHEN its line is rendered
    const out = html(decision({}));
    // THEN the card is the one the chat shows in that mode, with its full arguments behind
    expect(out).toContain('data-testid="approval-card-file_write"');
    expect(out).toContain('data-testid="approval-see-more-file_write"');
  });

  test("executing code offers two answers, and the whole command", async () => {
    // GIVEN a consent on a short command
    await inEnglish();
    const code = decision({
      access: "bash_executor",
      family: "execute_code",
      answers: ["once", "refuse"],
      code: "ls -la /tmp",
      workingDir: "/tmp",
    });
    // WHEN its line is rendered
    const out = html(code);
    // THEN the command is whole, once and refuse are offered, always is not
    expect(out).toContain("ls -la /tmp");
    expect(out).toContain('data-testid="operator-approval-accept-bash_executor"');
    expect(out).toContain('data-testid="operator-approval-refuse-bash_executor"');
    expect(out).not.toContain('data-testid="operator-approval-always-bash_executor"');
  });

  test("a long command is folded in the list and its answers wait for it to be shown", async () => {
    // GIVEN a consent on a command of fifteen lines
    await inEnglish();
    const long = Array.from({ length: 15 }, (_, i) => `echo step-${i + 1}`).join("\n");
    const code = decision({
      access: "bash_executor",
      family: "execute_code",
      answers: ["once", "refuse"],
      code: long,
    });
    // WHEN its line is rendered
    const out = html(code);
    // THEN four lines are shown with the link to the whole, and no answer yet
    expect(out).toContain("echo step-4");
    expect(out).not.toContain("echo step-5");
    expect(out).toContain("Command folded: 4 lines out of 15.");
    expect(out).toContain("Show the whole command");
    expect(out).not.toContain('data-testid="operator-approval-accept-bash_executor"');
  });

  test("a batch item and a paused task show the same card under their own origin", async () => {
    // GIVEN a batch item waiting for an access and a paused task
    await inEnglish();
    const batch = decision({
      id: "batch:b-1:13",
      kind: "batch_item",
      access: "send_email",
      family: "send_external",
      summary: "Waiting for the access send_email",
      origin: { batchId: "b-1", rank: 13 },
    });
    const pause = decision({
      id: "task:t-1",
      kind: "task_pause",
      access: null,
      summary: "Confirm sending the quote?",
      answers: ["once", "refuse"],
      origin: { taskId: "t-1" },
    });
    // WHEN their lines are rendered
    const batchOut = html(batch);
    const pauseOut = html(pause);
    // THEN each names its origin, says what it waits for, and shows the decision card
    expect(batchOut).toContain("From a batch, item 14");
    expect(batchOut).toContain("Waiting for the access send_email");
    expect(batchOut).toContain('data-testid="operator-approval-always-send_email"');
    expect(pauseOut).toContain("From a paused task");
    expect(batchOut).toContain("Open the batch");
    expect(pauseOut).toContain("Open the agent");
    expect(pauseOut).toContain("Confirm sending the quote?");
    expect(pauseOut).toContain('data-testid="operator-approval-accept-agent_action"');
    expect(pauseOut).not.toContain('data-testid="operator-approval-always-agent_action"');
  });

  test("a question shows the choices the agent proposes and another answer beside them", async () => {
    // GIVEN a question with two proposed choices
    await inEnglish();
    const question = decision({
      id: "question:r-1",
      kind: "question",
      summary: "Which format?",
      access: null,
      family: null,
      answers: ["answer", "refuse"],
      questions: [
        { id: "format", question: "Which format?", type: "single_choice", options: ["PDF", "Word"] },
      ],
      allowOther: true,
      origin: { sessionId: "s-2", requestId: "r-1" },
    });
    // WHEN its line is rendered
    const out = html(question);
    // THEN the question card offers both choices, a free answer, and declining
    expect(out).toContain("clerk asks you a question");
    expect(out).toContain("Answers proposed by the agent");
    expect(out).toContain('data-testid="review-question-radio-format-PDF"');
    expect(out).toContain('data-testid="review-question-radio-format-Word"');
    expect(out).toContain('data-testid="review-question-other-format"');
    expect(out).toContain("Decline to answer");
  });

  test("an answered item shrinks to its answer and offers Cancel", async () => {
    // GIVEN a consent answered "always" and not sent yet
    await inEnglish();
    const item = decision({});
    // WHEN its line is rendered
    const out = html(item, { id: item.id, answer: "always" });
    // THEN the card is gone, the answer is said, and Cancel is offered
    expect(out).toContain("Always allowed for clerk for this kind of action");
    expect(out).toContain("Answer recorded. This line leaves in a few seconds.");
    expect(out).toContain('data-testid="review-answered"');
    expect(out).toContain('data-testid="review-cancel"');
    expect(out).not.toContain('data-testid="operator-approval-file_write"');
  });
});
