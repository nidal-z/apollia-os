import { describe, test, expect } from "vitest";
import type { PendingDecision } from "$lib/ipc/pendingDecisions";
import {
  canAnswer,
  cardAnswers,
  collapsedCommand,
  consentRequest,
  emptyDraft,
  filterCounts,
  filterOf,
  newestFirst,
  originOf,
  questionFields,
  questionRequest,
  refusalRequest,
  shownUnder,
} from "./reviewModel";

function decision(over: Partial<PendingDecision>): PendingDecision {
  return {
    id: "consent:s-1::a-1::c-1",
    kind: "consent",
    agentId: "clerk",
    summary: "Tool 'file_write' asks to run",
    access: "file_write",
    family: "write_or_delete",
    answers: ["once", "always", "refuse"],
    questions: null,
    allowOther: false,
    origin: { sessionId: "s-1", messageId: "a-1", toolCallId: "c-1" },
    askedAt: "2026-10-03T09:00:00Z",
    ...over,
  };
}

const CONSENT = decision({});
const CODE = decision({
  id: "consent:s-1::a-1::c-2",
  access: "bash_executor",
  family: "execute_code",
  answers: ["once", "refuse"],
  code: "ls",
  askedAt: "2026-10-03T10:00:00Z",
});
const PAUSE = decision({
  id: "task:t-1",
  kind: "task_pause",
  access: null,
  family: null,
  answers: ["once", "refuse"],
  origin: { taskId: "t-1" },
  askedAt: "2026-10-02T09:00:00Z",
});
const BATCH = decision({
  id: "batch:b-1:13",
  kind: "batch_item",
  access: "send_email",
  family: "send_external",
  origin: { batchId: "b-1", rank: 13, taskId: "t-9" },
  askedAt: "2026-10-03T11:00:00Z",
});
const QUESTION = decision({
  id: "question:r-1",
  kind: "question",
  agentId: null,
  summary: "Which format?",
  access: null,
  family: null,
  answers: ["answer", "refuse"],
  questions: [
    { id: "format", question: "Which format?", type: "single_choice", options: ["PDF", "Word"] },
  ],
  allowOther: true,
  origin: { sessionId: "s-2", requestId: "r-1" },
  askedAt: null,
});
const ALL = [CONSENT, CODE, PAUSE, BATCH, QUESTION];

describe("To review - one list, a filter by type", () => {
  test("each source counts under its chip, a task pause under the decisions", () => {
    // GIVEN one item of every source the runtime gathers
    // WHEN each is filed under a chip
    const filed = ALL.map(filterOf);
    // THEN consents and the pause are decisions, the batch item and the question their own
    expect(filed).toEqual(["decisions", "decisions", "decisions", "batch_items", "questions"]);
    expect(filterCounts(ALL)).toEqual({ all: 5, decisions: 3, questions: 1, batch_items: 1 });
  });

  test("the list is one, the most recent first, an undated item last", () => {
    // GIVEN the items in the order the runtime lists them
    // WHEN they are ordered for the screen
    const ids = newestFirst(ALL).map((item) => item.id);
    // THEN the latest asked leads and the question with no date closes the list
    expect(ids).toEqual([BATCH.id, CODE.id, CONSENT.id, PAUSE.id, QUESTION.id]);
  });

  test("a chip shows only its items, and All shows every one", () => {
    // GIVEN the five items
    // WHEN a chip is picked
    // THEN the list keeps the items filed under it
    expect(shownUnder(ALL, "questions")).toEqual([QUESTION]);
    expect(shownUnder(ALL, "batch_items")).toEqual([BATCH]);
    expect(shownUnder(ALL, "all")).toHaveLength(5);
  });

  test("the typed question of a paused task is filed under the questions", () => {
    // GIVEN a task paused on a typed question
    const asked = decision({ ...PAUSE, answers: ["answer", "refuse"] });
    // WHEN it is filed
    // THEN it is answered with words, so it counts with the questions
    expect(filterOf(asked)).toBe("questions");
  });
});

describe("To review - where an item comes from", () => {
  test("each item names its origin, a batch item by its position from 1", () => {
    // GIVEN one item of every source
    // WHEN its origin line is resolved
    // THEN the conversation, the task and the batch are told apart
    expect(originOf(CONSENT)).toEqual({ kind: "conversation", sessionId: "s-1" });
    expect(originOf(PAUSE)).toEqual({ kind: "task", taskId: "t-1" });
    expect(originOf(BATCH)).toEqual({ kind: "batch", batchId: "b-1", position: 14 });
    expect(originOf(QUESTION)).toEqual({ kind: "conversation", sessionId: "s-2" });
    expect(originOf(decision({ origin: {} }))).toEqual({ kind: "unknown" });
  });
});

describe("To review - the answers of the decision card", () => {
  test("a consent keeps its three answers and code its two", () => {
    // GIVEN a consent on a tool and one on code
    // WHEN the answers of the card are read
    // THEN the card offers what the item takes, never "always" for code
    expect(cardAnswers(CONSENT)).toEqual(["once", "always", "refuse"]);
    expect(cardAnswers(CODE)).toEqual(["once", "refuse"]);
  });

  test("each answer of the card becomes the request the one route takes", () => {
    // GIVEN the three decisions the chat card emits
    // WHEN each is turned into a request for the item
    const once = consentRequest(CONSENT, { decision: "accept", reason: null });
    const always = consentRequest(CONSENT, { decision: "always_accept", reason: null });
    const refuse = consentRequest(CONSENT, { decision: "refuse", reason: "not this file" });
    // THEN the item is named and the answer is one of those it takes
    expect(once).toEqual({ id: CONSENT.id, answer: "once" });
    expect(always).toEqual({ id: CONSENT.id, answer: "always" });
    expect(refuse).toEqual({ id: CONSENT.id, answer: "refuse", reason: "not this file" });
  });

  test("a long command is folded to its first lines, a short one is whole", () => {
    // GIVEN a command of fifteen lines and one of two
    const long = Array.from({ length: 15 }, (_, i) => `line ${i + 1}`).join("\n");
    // WHEN each is prepared for the list
    const folded = collapsedCommand(long);
    const whole = collapsedCommand("cd /tmp\nls");
    // THEN four lines of fifteen are shown, and the short one is not folded
    expect(folded).toEqual({
      text: "line 1\nline 2\nline 3\nline 4",
      shown: 4,
      total: 15,
      collapsed: true,
    });
    expect(whole.collapsed).toBe(false);
    expect(whole.text).toBe("cd /tmp\nls");
  });
});

describe("To review - a question is answered as the agent asked it", () => {
  test("the choices the agent proposes are offered, and nothing is sent empty", () => {
    // GIVEN a question with two proposed choices
    const fields = questionFields(QUESTION);
    // WHEN nothing is picked yet
    // THEN the choices are there and there is nothing to send
    expect(fields).toHaveLength(1);
    expect(fields[0].kind).toBe("single");
    expect(fields[0].options.map((o) => o.value)).toEqual(["PDF", "Word"]);
    expect(canAnswer(fields, emptyDraft())).toBe(false);
  });

  test("a picked choice is sent, and another answer typed wins over it", () => {
    // GIVEN the question, a choice picked, then another answer typed
    const fields = questionFields(QUESTION);
    const picked = { picked: { format: ["PDF"] }, other: {} };
    const typed = { picked: { format: ["PDF"] }, other: { format: " Markdown " } };
    // WHEN each draft is turned into a request
    const first = questionRequest(QUESTION, fields, picked);
    const second = questionRequest(QUESTION, fields, typed);
    // THEN the choice, then the free answer, travels under the question id
    expect(first).toEqual({
      id: QUESTION.id,
      answer: "answer",
      answers: [{ id: "format", value: "PDF", values: [], skipped: false }],
    });
    expect(second.answers).toEqual([
      { id: "format", value: "Markdown", values: [], skipped: false },
    ]);
  });

  test("the typed question of a paused task sends a proposition, a number or a yes", () => {
    // GIVEN three paused tasks: a choice, a bound and a confirmation
    const pause = (questions: unknown) =>
      decision({ ...PAUSE, answers: ["answer", "refuse"], allowOther: true, questions });
    const choice = pause({
      genre: "choix",
      question: "Which source?",
      propositions: [{ id: "crm", libelle: "The CRM" }],
    });
    const bound = pause({ genre: "seuil", question: "Above which amount?" });
    const confirm = pause({ genre: "confirmation", question: "Send it?" });
    // WHEN each is answered
    const picked = questionRequest(choice, questionFields(choice), {
      picked: { answer: ["crm"] },
      other: {},
    });
    const number = questionRequest(bound, questionFields(bound), {
      picked: {},
      other: { answer: "1500,5" },
    });
    const yes = questionRequest(confirm, questionFields(confirm), {
      picked: { answer: ["true"] },
      other: {},
    });
    // THEN the value is the proposition id, a number, and a boolean
    expect(questionFields(choice)[0].options).toEqual([{ value: "crm", label: "The CRM" }]);
    expect(picked).toEqual({ id: choice.id, answer: "answer", value: "crm" });
    expect(number.value).toBe(1500.5);
    expect(questionFields(confirm)[0].kind).toBe("confirm");
    expect(yes.value).toBe(true);
  });

  test("declining carries the reason only when one was typed", () => {
    // GIVEN a question declined with and without a reason
    // WHEN the refusal is built
    // THEN an empty reason is no reason
    expect(refusalRequest(QUESTION, "  ")).toEqual({
      id: QUESTION.id,
      answer: "refuse",
      reason: null,
    });
    expect(refusalRequest(QUESTION, " later ").reason).toBe("later");
  });
});
