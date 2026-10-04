import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { CONSENT_ANSWERS, consentDecision } from "./ConsentAnswers.svelte";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

// DOM rendering is exercised by the gestural E2E layer (vitest runs in `node`).
// These tests lock what the consent card can send: three answers, no scope,
// and a refusal that acts at once with an optional reason.

/** The source of a component of this folder, to read what it renders. */
function source(file: string): string {
  return readFileSync(join(__dirname, file), "utf-8");
}

describe("ConsentAnswers - the three answers", () => {
  test("the card offers once, always for this agent and refuse, in that order", () => {
    // GIVEN the answers the card renders
    // WHEN they are listed
    const answers = [...CONSENT_ANSWERS];
    // THEN there are exactly three
    expect(answers).toEqual(["once", "always", "refuse"]);
  });

  test("once and always send no reason and no scope", () => {
    // GIVEN a reason typed in the field
    const typed = "not this one";
    // WHEN once or always is answered
    const once = consentDecision("once", typed);
    const always = consentDecision("always", typed);
    // THEN each sends its decision alone: the reason belongs to a refusal and
    // nothing names a scope
    expect(once).toEqual({ decision: "accept", reason: null });
    expect(always).toEqual({ decision: "always_accept", reason: null });
    expect(Object.keys(always)).not.toContain("scope");
  });

  test("a refusal carries the reason typed beside it, trimmed", () => {
    // GIVEN a reason with surrounding blanks
    // WHEN the operator refuses
    const decision = consentDecision("refuse", "  write a copy instead \n");
    // THEN the agent receives the reason as typed, without the blanks
    expect(decision).toEqual({ decision: "refuse", reason: "write a copy instead" });
  });

  test("a refusal without a reason acts at once", () => {
    // GIVEN an empty or blank reason field
    // WHEN the operator refuses
    const empty = consentDecision("refuse", "");
    const blank = consentDecision("refuse", "   ");
    // THEN the refusal goes through with no reason rather than waiting for one
    expect(empty).toEqual({ decision: "refuse", reason: null });
    expect(blank).toEqual({ decision: "refuse", reason: null });
  });
});

describe("Approval cards - no sub-menu, no dialog", () => {
  test.each(["OperatorApprovalCard.svelte", "ApprovalCard.svelte"])(
    "%s answers through ConsentAnswers and opens no scope menu",
    (file) => {
      // GIVEN the source of a chat approval card
      const text = source(file);
      // WHEN its answers are looked for
      // THEN it renders the three answers and nothing names a scope, a scope
      // menu or the reason dialog that used to stand between refuse and its effect
      expect(text).toContain("<ConsentAnswers");
      expect(text).not.toMatch(/scope/i);
      expect(text).not.toContain("RejectReasonDialog");
    },
  );

  test("the separate file prompt is gone from the conversation", () => {
    // GIVEN the conversation component
    const text = source("ChatConversation.svelte");
    // WHEN the prompts it mounts are read
    // THEN no file modal sits beside the card
    expect(text).not.toContain("HitlFilesystemModal");
  });
});

describe("ConsentAnswers - wording", () => {
  test.each([
    ["fr", fr],
    ["en", en],
  ])("%s carries every label the card shows", (_locale, catalogue) => {
    // GIVEN a catalogue
    const approval = (catalogue as { approval: Record<string, unknown> }).approval;
    const action = approval.action as Record<string, string>;
    // WHEN the card's keys are read
    // THEN each one has a non-empty text, and no scope label is left
    for (const value of [
      action.once,
      action.always_for_agent,
      action.refuse,
      approval.always_hint,
      approval.refuse_reason_placeholder,
    ]) {
      expect(typeof value === "string" && value.length > 0).toBe(true);
    }
    expect(approval.scope).toBeUndefined();
  });
});
