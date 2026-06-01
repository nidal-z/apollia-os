import { describe, test, expect } from "vitest";
import { buildAskUserAnswers, type AskUserFormState } from "./askUserForm";
import type { AskUserQuestion } from "$lib/types";

const EMPTY_STATE: AskUserFormState = { open: {}, single: {}, multi: {} };

describe("buildAskUserAnswers - three question types", () => {
  test("open question with text returns value, no skip", () => {
    // GIVEN one open-text question and a non-empty input
    const questions: AskUserQuestion[] = [
      { id: "stack", question: "Quelle stack ?", type: "open", options: [] },
    ];
    const state: AskUserFormState = {
      ...EMPTY_STATE,
      open: { stack: "  FastAPI  " },
    };
    // WHEN
    const answers = buildAskUserAnswers(questions, state);
    // THEN - value is trimmed, skipped is false
    expect(answers).toEqual([{ id: "stack", value: "FastAPI", skipped: false }]);
  });

  test("single_choice picks the selected option", () => {
    // GIVEN one radio question with two options
    const questions: AskUserQuestion[] = [
      {
        id: "lang",
        question: "Langue ?",
        type: "single_choice",
        options: ["FR", "EN"],
      },
    ];
    const state: AskUserFormState = { ...EMPTY_STATE, single: { lang: "FR" } };
    // WHEN
    const answers = buildAskUserAnswers(questions, state);
    // THEN
    expect(answers).toEqual([{ id: "lang", value: "FR", skipped: false }]);
  });

  test("multi_choice returns the array of checked options", () => {
    // GIVEN one multi-choice question with three options, two ticked
    const questions: AskUserQuestion[] = [
      {
        id: "tags",
        question: "Tags ?",
        type: "multi_choice",
        options: ["api", "ui", "db"],
      },
    ];
    const state: AskUserFormState = {
      ...EMPTY_STATE,
      multi: { tags: { api: true, ui: false, db: true } },
    };
    // WHEN
    const answers = buildAskUserAnswers(questions, state);
    // THEN
    expect(answers).toHaveLength(1);
    expect(answers[0].id).toBe("tags");
    expect(answers[0].skipped).toBe(false);
    expect(answers[0].values?.sort()).toEqual(["api", "db"]);
  });
});

describe("buildAskUserAnswers - skipped state (soft validation)", () => {
  test("empty open input is marked skipped", () => {
    const questions: AskUserQuestion[] = [
      { id: "q1", question: "?", type: "open", options: [] },
    ];
    const answers = buildAskUserAnswers(questions, EMPTY_STATE);
    expect(answers).toEqual([{ id: "q1", skipped: true }]);
  });

  test("whitespace-only open input is marked skipped", () => {
    const questions: AskUserQuestion[] = [
      { id: "q1", question: "?", type: "open", options: [] },
    ];
    const state: AskUserFormState = { ...EMPTY_STATE, open: { q1: "   \t  " } };
    const answers = buildAskUserAnswers(questions, state);
    expect(answers).toEqual([{ id: "q1", skipped: true }]);
  });

  test("unselected single_choice is marked skipped", () => {
    const questions: AskUserQuestion[] = [
      { id: "q1", question: "?", type: "single_choice", options: ["a", "b"] },
    ];
    const answers = buildAskUserAnswers(questions, EMPTY_STATE);
    expect(answers).toEqual([{ id: "q1", skipped: true }]);
  });

  test("multi_choice with no ticked option is marked skipped", () => {
    const questions: AskUserQuestion[] = [
      { id: "q1", question: "?", type: "multi_choice", options: ["a", "b"] },
    ];
    const state: AskUserFormState = {
      ...EMPTY_STATE,
      multi: { q1: { a: false, b: false } },
    };
    const answers = buildAskUserAnswers(questions, state);
    expect(answers).toEqual([{ id: "q1", skipped: true }]);
  });

  test("mixed: one answered + two skipped preserves order", () => {
    const questions: AskUserQuestion[] = [
      { id: "a", question: "open?", type: "open", options: [] },
      { id: "b", question: "single?", type: "single_choice", options: ["x", "y"] },
      { id: "c", question: "multi?", type: "multi_choice", options: ["1", "2"] },
    ];
    const state: AskUserFormState = {
      ...EMPTY_STATE,
      single: { b: "x" },
    };
    const answers = buildAskUserAnswers(questions, state);
    expect(answers).toHaveLength(3);
    expect(answers[0]).toEqual({ id: "a", skipped: true });
    expect(answers[1]).toEqual({ id: "b", value: "x", skipped: false });
    expect(answers[2]).toEqual({ id: "c", skipped: true });
  });
});
