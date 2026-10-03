import { describe, it, expect } from "vitest";
import {
  missingInputs,
  suggestedTriggerId,
  targetKindLabelKey,
  targetRequestFields,
  type AutomationTarget,
} from "./triggerTarget";

const weekly: AutomationTarget = {
  kind: "workflow",
  name: "reports/weekly",
  fields: [
    { name: "when", required: true },
    { name: "note", required: false },
  ],
};

describe("targetRequestFields", () => {
  it("sends an agent its message, and nothing of a workflow", () => {
    // GIVEN an agent target and a message with surrounding blanks
    const target: AutomationTarget = { kind: "agent", name: "veille" };

    // WHEN the request fields are built
    const fields = targetRequestFields(target, "  Daily watch  ", {});

    // THEN the agent and the trimmed message are set, no workflow field
    expect(fields).toEqual({ agent: "veille", input_template: "Daily watch" });
  });

  it("sends a workflow the inputs that hold a value", () => {
    // GIVEN a workflow target, one input filled and one left blank
    const inputs = { when: "{{fired_at}}", note: "   " };

    // WHEN the request fields are built
    const fields = targetRequestFields(weekly, "ignored", inputs);

    // THEN the workflow and its filled input are set, never a message
    expect(fields).toEqual({
      workflow: "reports/weekly",
      workflow_inputs: { when: "{{fired_at}}" },
    });
  });
});

describe("missingInputs", () => {
  it("names the required inputs still empty, and none for an agent", () => {
    // GIVEN a workflow with its required input empty, and an agent
    const agent: AutomationTarget = { kind: "agent", name: "veille" };

    // WHEN the missing inputs are read
    const forWorkflow = missingInputs(weekly, { note: "x" });
    const forAgent = missingInputs(agent, {});

    // THEN only the required one is named
    expect(forWorkflow).toEqual(["when"]);
    expect(forAgent).toEqual([]);
  });
});

describe("suggestedTriggerId", () => {
  it("slugs an agent or a workflow name with the source", () => {
    // GIVEN names of both kinds of target
    // WHEN an identifier is proposed for each
    // THEN it follows the grammar of the trigger store
    expect(suggestedTriggerId("Morning Digest", "cron")).toBe("morning-digest-cron");
    expect(suggestedTriggerId("reports/weekly", "file_watch")).toBe("reports-weekly-file-watch");
  });
});

describe("targetKindLabelKey", () => {
  it("uses the official word of each kind", () => {
    // GIVEN the two kinds
    // WHEN their label keys are read
    // THEN each has its own key
    expect(targetKindLabelKey("agent")).toBe("triggers.agent_label");
    expect(targetKindLabelKey("workflow")).toBe("triggers.workflow_label");
  });
});
