import { describe, expect, test } from "vitest";
import { failureText } from "./failureText";

const tr = (key: string, options?: { values: Record<string, string | number> }) =>
  options ? `${key} ${JSON.stringify(options.values)}` : key;

describe("the failure of a batch item in plain words", () => {
  test("a file not found says which, and where one of that name is", () => {
    // GIVEN the error of a read whose path was guessed
    const raw =
      "tool call failed: tool execution failed: not_found: not found: 'Devis/conditions.pdf'. A file of that name is in the folders granted to this agent: conditions.pdf";

    // WHEN it is said
    const said = failureText(raw, tr);

    // THEN the file and the place of the one found are named, the raw text kept
    expect(said.text).toBe('batch_view.failure.not_found {"path":"Devis/conditions.pdf"}');
    expect(said.hint).toBe('batch_view.failure.hint {"paths":"conditions.pdf"}');
    expect(said.raw).toBe(raw);
  });

  test("a plan that could not be redone, a missing tool and a deadline read as such", () => {
    // GIVEN three runtime failures
    // WHEN they are said
    // THEN each reads as what happened
    expect(failureText("internal error: REPLAN_FAILED: plan parse/validation failed after 3 attempts", tr).text).toBe(
      "batch_view.failure.replan",
    );
    expect(failureText("unauthorized: tool 'read_file' is not granted to this agent", tr).text).toBe(
      'batch_view.failure.tool {"tool":"read_file"}',
    );
    expect(failureText("the item ran past 20 minutes and was stopped", tr).text).toBe(
      'batch_view.failure.deadline {"minutes":"20"}',
    );
  });

  test("an unknown failure is shown as it came", () => {
    // GIVEN a failure no rule knows
    // WHEN it is said
    // THEN its raw text is the text
    expect(failureText("something else", tr).text).toBe("something else");
  });
});
