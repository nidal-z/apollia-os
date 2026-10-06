import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import type { FirstAgent } from "$lib/onboarding/firstAgent";
import type { ChatSessionSummary, CreateSessionRequest } from "$lib/types";
import {
  firstExampleOf,
  firstTaskPlan,
  openFirstConversation,
  sessionRequestOf,
  type FirstConversation,
  type FirstConversationSteps,
  type FirstTaskIntro,
} from "./firstTask";

function source(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

const WATCH: FirstAgent = { id: "web-watch", name: "Web watch", way: "example", packageName: "web-watch" };
const QUALIFY: FirstAgent = {
  id: "qualify-requests",
  name: "Qualify requests",
  way: "example",
  packageName: "qualify-requests",
};
const CREATED: FirstAgent = { id: "my-agent", name: "My agent", way: "create", packageName: null };

interface Played {
  calls: string[];
  requests: CreateSessionRequest[];
  prefilled: string[];
  intros: FirstTaskIntro[];
  reported: unknown[];
}

function steps(over: Partial<FirstConversationSteps> = {}): { steps: FirstConversationSteps; played: Played } {
  const played: Played = { calls: [], requests: [], prefilled: [], intros: [], reported: [] };
  const base: FirstConversationSteps = {
    createSession: async (request) => {
      played.calls.push("create");
      played.requests.push(request);
      return { id: "session-1" } as ChatSessionSummary;
    },
    agentExamples: async () => {
      played.calls.push("examples");
      return ["  Watch what our three main competitors announced this week.  ", "Another one"];
    },
    rememberSession: (sessionId) => {
      played.calls.push(`remember:${sessionId}`);
    },
    navigate: () => {
      played.calls.push("navigate");
    },
    prefill: (text) => {
      played.calls.push("prefill");
      played.prefilled.push(text);
    },
    introduce: (intro) => {
      played.calls.push("introduce");
      played.intros.push(intro);
    },
    report: (err) => {
      played.calls.push("report");
      played.reported.push(err);
    },
  };
  return { steps: { ...base, ...over }, played };
}

describe("first task - what the first launch closes on", () => {
  test("with an agent kept, a conversation with that agent", () => {
    // GIVEN an agent kept at the first-agent step, from an example without a dataset
    // WHEN the first task is decided
    const plan = firstTaskPlan(WATCH);

    // THEN it is a conversation with that agent, named
    expect(plan).toEqual({ kind: "conversation", agentId: "web-watch", agentName: "Web watch" });
  });

  test("an agent created in the step gets its conversation too", () => {
    // GIVEN an agent created by describing the need, which comes from no package
    // WHEN the first task is decided
    const plan = firstTaskPlan(CREATED);

    // THEN it is a conversation with it
    expect(plan).toEqual({ kind: "conversation", agentId: "my-agent", agentName: "My agent" });
  });

  test("for the example that qualifies requests, the trial on its demonstration dataset", () => {
    // GIVEN the agent of the example that comes with a demonstration dataset
    // WHEN the first task is decided
    const plan = firstTaskPlan(QUALIFY);

    // THEN it is the trial of that example with that agent, which copies the
    // dataset and launches a batch on its workbook, and the conversation with
    // the agent is what it falls back on
    expect(plan).toEqual({
      kind: "trial",
      trial: { example: "qualify-requests", agent: "qualify-requests" },
      fallback: { kind: "conversation", agentId: "qualify-requests", agentName: "Qualify requests" },
    });
  });

  test("a package installed from a file under another name is not the example", () => {
    // GIVEN an agent whose package is not one the application carries a dataset for
    const other: FirstAgent = { ...QUALIFY, packageName: "my-qualifier", way: "install" };

    // WHEN the first task is decided
    // THEN it is a conversation: there is nothing to copy
    expect(firstTaskPlan(other).kind).toBe("conversation");
  });

  test("without an agent, a conversation with Apollia", () => {
    // GIVEN a first-agent step that was passed
    // WHEN the first task is decided
    const plan = firstTaskPlan(null);

    // THEN it is a conversation with the agent every installation has,
    // which needs no introduction
    expect(plan).toEqual({ kind: "conversation", agentId: APOLLIA_AGENT_ID, agentName: null });
  });
});

describe("first task - the session of the conversation", () => {
  test("an agent kept gets a session bound to it", () => {
    // GIVEN the conversation with an agent kept
    const conversation = firstTaskPlan(WATCH) as FirstConversation;

    // WHEN its session is asked for
    // THEN it is a session of that agent
    expect(sessionRequestOf(conversation)).toEqual({ mode: "agent", agent_name: "web-watch" });
  });

  test("Apollia is reached by a free conversation", () => {
    // GIVEN the conversation of a first launch without an agent
    const conversation = firstTaskPlan(null) as FirstConversation;

    // WHEN its session is asked for
    // THEN it is a free one, the way the built-in agent is talked to
    expect(sessionRequestOf(conversation)).toEqual({ mode: "libre" });
  });
});

describe("first task - the first example of an agent", () => {
  test("it is the first one that says something, trimmed", () => {
    // GIVEN the examples an agent declares, the first ones empty
    // WHEN the first example is read
    // THEN it is the first with text, without the spaces around it
    expect(firstExampleOf(["", "   ", "  Sort my mail  ", "Other"])).toBe("Sort my mail");
  });

  test("an agent that declares none has none", () => {
    // GIVEN no example, an empty list, or only blank ones
    // WHEN the first example is read
    // THEN there is none
    expect(firstExampleOf(null)).toBeNull();
    expect(firstExampleOf(undefined)).toBeNull();
    expect(firstExampleOf([])).toBeNull();
    expect(firstExampleOf([" "])).toBeNull();
  });
});

describe("first task - opening the conversation", () => {
  test("the composer holds the first example of the agent, and nothing is sent", async () => {
    // GIVEN the conversation with an agent that declares examples
    const { steps: calls, played } = steps();

    // WHEN it is opened
    const opened = await openFirstConversation(calls, firstTaskPlan(WATCH) as FirstConversation);

    // THEN the session is created for the agent and handed to the chat screen
    // before the composer is written in
    expect(opened).toBe(true);
    expect(played.requests).toEqual([{ mode: "agent", agent_name: "web-watch" }]);
    expect(played.calls).toEqual(["create", "remember:session-1", "navigate", "examples", "prefill", "introduce"]);
    // AND the composer holds its first example, the conversation says so
    expect(played.prefilled).toEqual(["Watch what our three main competitors announced this week."]);
    expect(played.intros).toEqual([{ sessionId: "session-1", agentName: "Web watch", prefilled: true }]);
  });

  test("an agent without an example gets its conversation, with an empty composer", async () => {
    // GIVEN an agent that declares no example
    const { steps: calls, played } = steps({ agentExamples: async () => [] });

    // WHEN its conversation is opened
    const opened = await openFirstConversation(calls, firstTaskPlan(CREATED) as FirstConversation);

    // THEN the conversation is open, nothing is written, and it does not
    // claim an example
    expect(opened).toBe(true);
    expect(played.prefilled).toEqual([]);
    expect(played.intros).toEqual([{ sessionId: "session-1", agentName: "My agent", prefilled: false }]);
  });

  test("examples that cannot be read do not cost the conversation", async () => {
    // GIVEN an agent whose definition is refused
    const { steps: calls, played } = steps({
      agentExamples: async () => {
        throw new Error("unreachable");
      },
    });

    // WHEN its conversation is opened
    const opened = await openFirstConversation(calls, firstTaskPlan(WATCH) as FirstConversation);

    // THEN it is open all the same, with an empty composer and no error shown
    expect(opened).toBe(true);
    expect(played.prefilled).toEqual([]);
    expect(played.reported).toEqual([]);
    expect(played.intros[0]?.prefilled).toBe(false);
  });

  test("with Apollia, the conversation opens bare", async () => {
    // GIVEN a first launch without an agent
    const { steps: calls, played } = steps();

    // WHEN its conversation is opened
    const opened = await openFirstConversation(calls, firstTaskPlan(null) as FirstConversation);

    // THEN a free conversation is open, no example is looked for, nothing is
    // written and nothing introduces it
    expect(opened).toBe(true);
    expect(played.requests).toEqual([{ mode: "libre" }]);
    expect(played.calls).toEqual(["create", "remember:session-1", "navigate"]);
  });

  test("a refused creation is reported, and nothing is opened", async () => {
    // GIVEN a backend that refuses the session
    const refusal = new Error("no language model");
    const { steps: calls, played } = steps({
      createSession: async () => {
        throw refusal;
      },
    });

    // WHEN the conversation is opened
    const opened = await openFirstConversation(calls, firstTaskPlan(WATCH) as FirstConversation);

    // THEN the refusal is shown and the person is not sent to an empty screen
    expect(opened).toBe(false);
    expect(played.reported).toEqual([refusal]);
    expect(played.calls).toEqual(["report"]);
  });
});

describe("first task - the application plays it once the modal is closed", () => {
  const host = source("./OnboardingFirstTask.svelte");

  test("the trial goes through the copy dialog, then the launch of a batch on the copied workbook", () => {
    // GIVEN the component that plays the first task
    // WHEN its two dialogs are read
    // THEN the copy dialog hands the workbook it copied to the launch dialog,
    // which opens on that file and shows its preview
    expect(host).toContain("<ExampleTrialDialog {trial} onclose={leave} onLaunchBatch={launchTrialBatch} />");
    expect(host).toContain("trialBatch = { agent, workbook };");
    expect(host).toContain("file={trialBatch?.workbook ?? null}");
    expect(host).toContain("agent={trialBatch?.agent ?? null}");
    expect(host).toContain('openBatch(batch.batchId, "launch");');
    const launch = source("../batch/BatchLaunchDialog.svelte");
    expect(launch).toContain("<WorkbookPreview");
  });

  test("leaving the copy or the launch falls back on the conversation, a launched batch does not", () => {
    // GIVEN the same component
    const leave = host.slice(host.indexOf("function leave(): void {"), host.indexOf("function launchTrialBatch"));

    // WHEN what leaving a dialog does is read
    // THEN the conversation with the agent opens instead, nothing being copied
    expect(leave).toContain("if (conversation !== null) void converse(conversation);");
    expect(host).toContain("fallback = plan.fallback;");
    // AND a batch that was launched clears the fallback before the dialog closes
    const launched = host.slice(host.indexOf("onlaunched={(batch) => {"));
    expect(launched.indexOf("fallback = null;")).toBeGreaterThan(-1);
    expect(launched.indexOf("fallback = null;")).toBeLessThan(launched.indexOf("openBatch("));
  });

  test("it is mounted above every screen, and takes each request once", () => {
    // GIVEN the application shell and the component
    const app = source("../../App.svelte");

    // WHEN both are read
    // THEN the shell mounts it outside the modal, and a request is cleared as it is taken
    expect(app).toContain("<OnboardingFirstTask />");
    expect(app.indexOf("<OnboardingFirstTask />")).toBeLessThan(app.indexOf("{#if $onboardingModalOpen}"));
    expect(host).toContain("firstTaskRequest.set(null);");
  });

  test("the conversation says what it is, in both languages", () => {
    // GIVEN the empty state of a conversation and the two catalogues
    const scroller = source("../chat/ChatMessageScroller.svelte");

    // WHEN the introduction of the first conversation is read
    // THEN it is shown for that session only, and its sentences exist
    expect(scroller).toContain("{#if $firstTaskIntro?.sessionId === sessionId}");
    expect(scroller).toContain('data-testid="first-task-intro"');
    for (const catalogue of [fr, en]) {
      const texts = catalogue.onboarding.first_task;
      expect(texts.intro_title).toContain("{name}");
      expect(texts.intro_body.trim()).toBeTruthy();
    }
  });
});
