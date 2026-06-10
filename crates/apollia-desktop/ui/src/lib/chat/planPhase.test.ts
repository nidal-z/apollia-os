import { describe, test, expect } from "vitest";
import { phaseLabelKey } from "./planPhase";

describe("phaseLabelKey", () => {
  test("maps every known phase to its i18n key", () => {
    // GIVEN each plan-mode phase
    // WHEN the label key is resolved
    // THEN it points at the matching i18n entry
    expect(phaseLabelKey("discovery")).toBe("chat.planMode.phase.discovery");
    expect(phaseLabelKey("drafting")).toBe("chat.planMode.phase.drafting");
    expect(phaseLabelKey("awaiting_approval")).toBe(
      "chat.planMode.phase.awaitingApproval",
    );
    expect(phaseLabelKey("executing")).toBe("chat.planMode.phase.executing");
    expect(phaseLabelKey("done")).toBe("chat.planMode.phase.done");
  });
});
