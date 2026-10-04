import { describe, expect, it } from "vitest";
import type { WorkflowRun, WorkflowStep } from "$lib/ipc/workflows";
import {
  elapsedMs,
  familyLabelKey,
  filterWorkflows,
  inputsSummary,
  lastRun,
  progressOf,
  runLabelKey,
  runningRuns,
  runTone,
  shownRuns,
  splitWorkflowId,
  stepLabelKey,
  stepPosition,
  stepTone,
  workflowAccess,
} from "./workflowRuns";

function run(runId: string, status: string, more: Partial<WorkflowRun> = {}): WorkflowRun {
  return {
    runId,
    workflowId: "sales/follow-up",
    parentRunId: null,
    depth: 1,
    inputs: {},
    status,
    output: null,
    error: null,
    startedAt: "2026-10-04T09:00:00Z",
    endedAt: null,
    ...more,
  };
}

function step(rank: number, status: string): WorkflowStep {
  return { rank, name: `step-${rank}`, status, startedAt: null, endedAt: null, output: null, error: null };
}

describe("the history of a workflow", () => {
  const runs = [
    run("r5", "running"),
    run("r4", "canceled"),
    run("r3", "failed"),
    run("r2", "completed"),
    run("r1", "completed"),
  ];

  it("names and tones each outcome", () => {
    // GIVEN the four outcomes of a run and one the interface does not know
    const statuses = ["running", "completed", "failed", "canceled", "paused"];

    // WHEN each is named and toned
    // THEN each has its own word and tone, the unknown one a neutral fallback
    expect(statuses.map(runLabelKey)).toEqual([
      "workflows.run_running",
      "workflows.run_completed",
      "workflows.run_failed",
      "workflows.run_canceled",
      "workflows.run_unknown",
    ]);
    expect(statuses.map(runTone)).toEqual(["info", "success", "danger", "warning", "neutral"]);
  });

  it("shows the first runs, then all of them once asked", () => {
    // GIVEN five runs, newest first
    // WHEN the history is shown folded, then in full
    // THEN the fold keeps the four most recent, in order
    expect(shownRuns(runs, false, 4).map((r) => r.runId)).toEqual(["r5", "r4", "r3", "r2"]);
    expect(shownRuns(runs, true, 4)).toHaveLength(5);
  });

  it("finds the most recent run and the runs still going", () => {
    // GIVEN a history whose newest run is still going
    // WHEN the last run and the runs going are asked
    // THEN the newest is the last, and it is the only one going
    expect(lastRun(runs)?.runId).toBe("r5");
    expect(runningRuns(runs).map((r) => r.runId)).toEqual(["r5"]);
    expect(lastRun([])).toBeNull();
  });

  it("reads the inputs of a run on one line", () => {
    // GIVEN inputs of several types
    const inputs = { client: "Acme", delay: 7, tags: ["a", "b"], notify: false };

    // WHEN they are summarised
    // THEN each is named with its value, lists joined
    expect(inputsSummary(inputs)).toBe("client: Acme · delay: 7 · tags: a, b · notify: false");
    expect(inputsSummary({})).toBe("");
    expect(inputsSummary(null)).toBe("");
  });

  it("measures a run to its end, or to now while it is going", () => {
    // GIVEN a start, an end ninety seconds later, and a clock two minutes later
    const start = "2026-10-04T09:00:00Z";
    const end = "2026-10-04T09:01:30Z";
    const now = Date.parse("2026-10-04T09:02:00Z");

    // WHEN an ended and a going span are measured
    // THEN the first stops at its end, the second runs to the clock
    expect(elapsedMs(start, end, now)).toBe(90_000);
    expect(elapsedMs(start, null, now)).toBe(120_000);
    expect(elapsedMs(null, null, now)).toBeNull();
    expect(elapsedMs("not a date", null, now)).toBeNull();
  });
});

describe("the steps of a run", () => {
  it("names and tones each state", () => {
    // GIVEN the six states of a step
    const statuses = ["pending", "running", "completed", "failed", "canceled", "not_started"];

    // WHEN each is named and toned
    // THEN a step to come and a step not launched are neutral, the others follow the run
    expect(statuses.map(stepLabelKey)).toEqual([
      "workflows.step_pending",
      "workflows.step_running",
      "workflows.step_completed",
      "workflows.step_failed",
      "workflows.step_canceled",
      "workflows.step_not_started",
    ]);
    expect(statuses.map(stepTone)).toEqual(["neutral", "info", "success", "danger", "warning", "neutral"]);
  });

  it("places a run at the step in progress", () => {
    // GIVEN three steps given out of order, the second in progress
    const steps = [step(2, "pending"), step(0, "completed"), step(1, "running")];

    // WHEN the position and the progress are asked
    // THEN the run is at step two of three, one third done
    expect(stepPosition(steps)).toEqual({ current: 2, total: 3 });
    expect(progressOf(steps)).toBe(33);
  });

  it("counts the steps done when none is in progress", () => {
    // GIVEN a stopped run: one step done, one stopped, one never launched
    const steps = [step(0, "completed"), step(1, "canceled"), step(2, "not_started")];

    // WHEN the position is asked
    // THEN it is the number of steps done
    expect(stepPosition(steps)).toEqual({ current: 1, total: 3 });
    expect(progressOf([])).toBe(0);
  });
});

describe("what a workflow may do", () => {
  const description = {
    workflows: [
      { name: "follow-up", action: "send_external" },
      { name: "audit", action: "read" },
      { name: "odd", action: "teleport" },
    ],
  };

  it("reads the action family its package declares", () => {
    // GIVEN the description of a package declaring three workflows
    // WHEN the access of each is read
    // THEN reading alone is not sensitive, sending outside is
    expect(workflowAccess(description, "follow-up")).toEqual({ family: "send_external", sensitive: true });
    expect(workflowAccess(description, "audit")).toEqual({ family: "read", sensitive: false });
  });

  it("says nothing when the package does not describe the workflow", () => {
    // GIVEN a workflow absent from the description, a family nobody knows, no description
    // WHEN the access is read
    // THEN nothing is claimed
    expect(workflowAccess(description, "missing")).toBeNull();
    expect(workflowAccess(description, "odd")).toBeNull();
    expect(workflowAccess(null, "audit")).toBeNull();
    expect(workflowAccess({ workflows: "none" }, "audit")).toBeNull();
  });

  it("names a family with the words of the sheet of an agent", () => {
    // GIVEN a known and an unknown family
    // WHEN each is named
    // THEN the known one has its key, the other the unclassified one
    expect(familyLabelKey("write_or_delete")).toBe("agent_sheet.family_write_or_delete");
    expect(familyLabelKey("teleport")).toBe("agent_sheet.family_unclassified");
  });
});

describe("the list of workflows", () => {
  const workflows = [
    { workflowId: "sales/follow-up", name: "Follow up", description: "Reminds the quotes left unanswered" },
    { workflowId: "billing/close", name: "Close the month", description: "Sorts the invoices" },
  ];

  it("keeps the workflows whose name, identifier or description holds the query", () => {
    // GIVEN two workflows
    // WHEN the list is filtered by a word of a name, of an identifier, of a description
    // THEN each query keeps the workflow that holds it, whatever its case
    expect(filterWorkflows(workflows, "MONTH").map((w) => w.workflowId)).toEqual(["billing/close"]);
    expect(filterWorkflows(workflows, "sales/").map((w) => w.workflowId)).toEqual(["sales/follow-up"]);
    expect(filterWorkflows(workflows, "invoices").map((w) => w.workflowId)).toEqual(["billing/close"]);
    expect(filterWorkflows(workflows, "  ")).toHaveLength(2);
    expect(filterWorkflows(workflows, "zzz")).toEqual([]);
  });

  it("splits an identifier into its package and its name", () => {
    // GIVEN an identifier with its package, and a bare name
    // WHEN each is split
    // THEN the package is found, or left empty
    expect(splitWorkflowId("sales/follow-up")).toEqual({ package: "sales", name: "follow-up" });
    expect(splitWorkflowId("follow-up")).toEqual({ package: "", name: "follow-up" });
  });
});
