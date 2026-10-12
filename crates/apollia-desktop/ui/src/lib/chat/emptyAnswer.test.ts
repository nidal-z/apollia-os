import { describe, expect, it } from "vitest";
import type { ToolCallView } from "$lib/types/chat";
import { isEmptyAnswer } from "./emptyAnswer";

function call(tool_name: string, status: ToolCallView["status"]): ToolCallView {
  return { tool_name, input: {}, output: null, status };
}

describe("isEmptyAnswer", () => {
  it("holds for an agent turn with no text, no button and no plan", () => {
    // GIVEN a finished agent turn that only ran a tool that failed
    const calls = [call("web_search", "failed")];
    // WHEN it is read
    const empty = isEmptyAnswer({ isUser: false, text: "  ", buttons: 0, calls });
    // THEN it is an exchange that produced nothing
    expect(empty).toBe(true);
  });

  it("does not hold for a turn that proposed a plan", () => {
    // GIVEN a turn that ends without text after proposing and submitting a plan
    const calls = [call("plan_propose", "executed"), call("plan_submit", "executed")];
    // WHEN it is read
    const empty = isEmptyAnswer({ isUser: false, text: "", buttons: 0, calls });
    // THEN the plan card is its answer
    expect(empty).toBe(false);
  });

  it("still holds when the plan proposal itself failed", () => {
    // GIVEN a turn whose only plan call was refused by the runtime
    const calls = [call("plan_propose", "failed")];
    // WHEN it is read
    const empty = isEmptyAnswer({ isUser: false, text: "", buttons: 0, calls });
    // THEN nothing was laid before the user
    expect(empty).toBe(true);
  });

  it("never holds for a user turn or a turn with text", () => {
    // GIVEN a user turn without text, and an agent turn with text
    // WHEN each is read
    // THEN neither is an empty answer
    expect(isEmptyAnswer({ isUser: true, text: "", buttons: 0, calls: [] })).toBe(false);
    expect(isEmptyAnswer({ isUser: false, text: "Done.", buttons: 0, calls: [] })).toBe(false);
  });
});
