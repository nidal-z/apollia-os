import { describe, test, expect, vi } from "vitest";
import type { ResolvedAgent } from "$lib/ipc/agentSheet";
import type { DraftOffer } from "$lib/ipc/agentCreate";
import type { PendingDecision } from "$lib/ipc/pendingDecisions";
import type { WorkflowStep } from "$lib/ipc/workflows";
import type { ModelOption } from "../sheet/agentAdjust";
import {
  WORKSHOP_WORKFLOW,
  canPrepare,
  canSave,
  draftOfChecked,
  emptyForm,
  offeredRights,
  questionOf,
  reviewOf,
  saveDraft,
  sheetOf,
  sheetOfRun,
  stepViews,
  unavailableViews,
  workshopInputs,
  type ProposedAgent,
  type WorkshopForm,
} from "./agentDraft";

function proposed(over: Partial<ProposedAgent> = {}): ProposedAgent {
  return {
    name: "Press watch",
    mission: "Summarise the press every morning.",
    examples: ["What is new today?"],
    model: null,
    access: [
      { id: "notes_lookup", checked: true, sensitive: false, by_default: false },
      { id: "mail_send", checked: false, sensitive: true, by_default: false },
      { id: "web_search", checked: true, sensitive: false, by_default: true },
    ],
    sources: ["profile"],
    unavailable: [],
    ...over,
  };
}

const OFFER: DraftOffer = {
  accesses: [
    { id: "file_read", description: "Read a file.", family: "read", sensitive: false, by_default: true },
    { id: "mail_send", description: "Send a mail.", family: "send_external", sensitive: true, by_default: false },
    { id: "file_write", description: "Write a file.", family: "write_or_delete", sensitive: true, by_default: false },
  ],
  sources: [
    { id: "profile", label: "the profile of the person" },
    { id: "src-notes", label: "Notes" },
  ],
  models: ["qwen"],
};

function filled(over: Partial<WorkshopForm> = {}): WorkshopForm {
  return { ...emptyForm(), purpose: "Read the quotes of suppliers.", ...over };
}

function step(name: string, status: WorkflowStep["status"], rank = 0): WorkflowStep {
  return { rank, name, status, startedAt: null, endedAt: null, output: null, error: null };
}

function decision(over: Partial<PendingDecision> = {}): PendingDecision {
  return {
    id: "workflow-question:r1",
    kind: "question",
    agentId: null,
    summary: "Which form?",
    access: null,
    family: null,
    answers: ["answer", "refuse"],
    questions: [{ id: "answer", question: "Which form?", type: "single_choice", options: ["A table", "A list"] }],
    allowOther: true,
    origin: { requestId: "r1", runId: "run-1", workflowId: WORKSHOP_WORKFLOW, step: "ask" },
    askedAt: null,
    ...over,
  };
}

const OPTIONS: ModelOption[] = [
  { id: "local", source: "local", model: "qwen" },
  { id: "remote", source: "server", model: "big" },
];

function created(over: Partial<ResolvedAgent> = {}): ResolvedAgent {
  return {
    id: "press-watch",
    origin: "interface",
    package: null,
    version: "1",
    enabled: true,
    name: "Press watch",
    mission: "Summarise the press every morning.",
    examples: [],
    access: [],
    sources: [],
    model: null,
    autonomy: "supervised",
    profile: null,
    overridden: [],
    ...over,
  };
}

describe("the form the workshop is launched on", () => {
  test("only what the agent is for is needed to launch", () => {
    // GIVEN an empty form, one with blanks, and one saying what the agent is for
    // WHEN asking whether each can be launched
    // THEN only the last one can
    expect(canPrepare(emptyForm())).toBe(false);
    expect(canPrepare(filled({ purpose: "   " }))).toBe(false);
    expect(canPrepare(filled())).toBe(true);
  });

  test("the rights offered are the families a tool of the machine belongs to", () => {
    // GIVEN a machine holding a reading, a sending and a writing tool
    // WHEN the rights of the form are read, then those of a machine that says nothing
    // THEN sending and writing are offered, in the fixed order, and nothing without an offer
    expect(offeredRights(OFFER)).toEqual(["write_or_delete", "send_external"]);
    expect(offeredRights(null)).toEqual([]);
  });

  test("the inputs carry the filled fields, the language and what the machine holds", () => {
    // GIVEN a form with a result, a blank rule, a source and a right that exist,
    // and a source and a right the machine no longer offers
    const form = filled({
      purpose: "  Read the quotes of suppliers. ",
      result: "A table.",
      rules: "   ",
      sources: ["src-notes", "src-gone"],
      rights: ["send_external", "execute_code"],
    });

    // WHEN the inputs of the launch are built, the interface in French
    const inputs = workshopInputs(form, OFFER, "fr-FR");

    // THEN the blank field is left out, what the machine lacks too, and the
    // offer goes as JSON without the models
    expect(inputs).toEqual({
      purpose: "Read the quotes of suppliers.",
      result: "A table.",
      sources: ["src-notes"],
      rights: ["send_external"],
      language: "fr",
      offer: JSON.stringify({ accesses: OFFER.accesses, sources: OFFER.sources }),
    });
  });

  test("without an offer the launch carries the text and the language only", () => {
    // GIVEN a form with a source checked, on a machine that could not say what it holds
    // WHEN the inputs are built, the language unknown
    const inputs = workshopInputs(filled({ sources: ["profile"] }), null, null);

    // THEN no source, no right and no offer are sent, and the language is English
    expect(inputs).toEqual({ purpose: "Read the quotes of suppliers.", language: "en" });
  });
});

describe("what the workshop hands back", () => {
  test("the output of a finished run is read as a sheet", () => {
    // GIVEN the output of a run, with fields a sheet does not hold
    const output = {
      name: "Quote reader",
      mission: "You read quotes.",
      examples: ["Compare these."],
      access: ["web_search", 3],
      sources: ["profile"],
      answers: [{ question: "Which form?", answer: "A table" }],
    };

    // WHEN it is read, as an object then as the text of that object
    const sheet = sheetOfRun(output);

    // THEN the sheet holds the five fields the runtime checks, and nothing else
    expect(sheet).toEqual({
      name: "Quote reader",
      mission: "You read quotes.",
      examples: ["Compare these."],
      access: ["web_search"],
      sources: ["profile"],
    });
    expect(sheetOfRun(JSON.stringify(output))).toEqual(sheet);
  });

  test("an output without a name or a mission is not a sheet", () => {
    // GIVEN outputs that are nothing, not JSON, a list, or a sheet with a blank mission
    // WHEN each is read
    // THEN none is a sheet
    for (const output of [null, "not json", [], { name: "Quotes" }, { name: "Quotes", mission: "  " }]) {
      expect(sheetOfRun(output)).toBeNull();
    }
  });

  test("a checked sheet is read as a draft, a sensitive access never checked by the sheet", () => {
    // GIVEN the answer of the runtime, one sensitive access marked checked
    // without the default granting it
    const checked = {
      name: "Quotes",
      mission: "Read the quotes.",
      examples: [],
      model: "",
      access: [
        { id: "mail_send", checked: true, sensitive: true, by_default: false },
        { id: "file_read", checked: true, sensitive: false, by_default: true },
        { checked: true },
      ],
      sources: ["profile"],
      unavailable: ["source:src-gone"],
    };

    // WHEN it is read
    const draft = draftOfChecked(checked);

    // THEN the sensitive access is unchecked, the nameless one dropped, the
    // empty model read as the default
    expect(draft?.access).toEqual([
      { id: "mail_send", checked: false, sensitive: true, by_default: false },
      { id: "file_read", checked: true, sensitive: false, by_default: true },
    ]);
    expect(draft?.model).toBeNull();
    expect(draft?.unavailable).toEqual(["source:src-gone"]);
    expect(draftOfChecked({ name: "Quotes" })).toBeNull();
  });
});

describe("the run, while it goes on", () => {
  test("before the run reports anything, the first step is in progress", () => {
    // GIVEN a run that reported no step
    // WHEN its steps are shown
    // THEN the first is in progress and the others are to come
    expect(stepViews([]).map((v) => v.state)).toEqual(["current", "upcoming", "upcoming"]);
  });

  test("a step done is marked, the next one is in progress", () => {
    // GIVEN a run whose first step is done and second running
    const steps = [step("understand", "completed"), step("ask", "running", 1), step("write", "pending", 2)];

    // WHEN its steps are shown
    // THEN they read done, in progress, to come
    expect(stepViews(steps)).toEqual([
      { step: "understand", state: "done" },
      { step: "ask", state: "current" },
      { step: "write", state: "upcoming" },
    ]);
  });

  test("a step that failed says so", () => {
    // GIVEN a run whose first step failed
    // WHEN its steps are shown
    // THEN the first reads failed
    expect(stepViews([step("understand", "failed")])[0].state).toBe("failed");
  });

  test("the question shown is the one the run waits on", () => {
    // GIVEN the decisions waiting for the user: a consent, the question of
    // another run, and the question of this run
    const decisions = [
      decision({ id: "consent:1", kind: "consent", origin: { sessionId: "s1" } }),
      decision({ id: "workflow-question:other", origin: { runId: "run-2" } }),
      decision(),
    ];

    // WHEN the question of the run is read
    const question = questionOf(decisions, "run-1");

    // THEN it is the question of this run, with its proposed answers
    expect(question).toEqual({ id: "workflow-question:r1", question: "Which form?", options: ["A table", "A list"] });
    expect(questionOf(decisions, "run-3")).toBeNull();
  });

  test("a question without choices is asked open, by its summary when it has no text", () => {
    // GIVEN a waiting question whose payload holds nothing
    const waiting = decision({ questions: null, summary: "How do you want it?" });

    // WHEN it is read
    // THEN the summary is the question, with no proposed answer
    expect(questionOf([waiting], "run-1")).toEqual({
      id: "workflow-question:r1",
      question: "How do you want it?",
      options: [],
    });
  });
});

describe("reviewOf - the draft opens in the form of a sheet", () => {
  test("the form lists every access and checks only the ones that are not sensitive", () => {
    // GIVEN a draft with an asked access, a sensitive one and a default one
    // WHEN the form is opened
    const review = reviewOf(proposed(), OPTIONS);

    // THEN the three are listed, the sensitive one unchecked, each with its note
    expect(review.listed).toEqual(["notes_lookup", "mail_send", "web_search"]);
    expect(review.form.access).toEqual(["notes_lookup", "web_search"]);
    expect(review.checked).toEqual(["notes_lookup", "web_search"]);
    expect(review.notes).toEqual({ mail_send: "sensitive", web_search: "by_default" });
  });

  test("the form holds the name, the mission, the examples and the sources of the draft", () => {
    // GIVEN a draft with blanks around its texts
    const draft = proposed({ name: " Press watch ", examples: ["One", " ", "Two"] });

    // WHEN the form is opened
    const review = reviewOf(draft, OPTIONS);

    // THEN the texts are trimmed and the empty example dropped
    expect(review.name).toBe("Press watch");
    expect(review.form.mission).toBe("Summarise the press every morning.");
    expect(review.form.examples).toEqual(["One", "Two"]);
    expect(review.form.sources).toEqual(["profile"]);
  });

  test("a model the machine offers is kept, another one falls back on the default", () => {
    // GIVEN a draft naming an offered model, then one naming an unknown model
    // WHEN each form is opened
    const offered = reviewOf(proposed({ model: "remote" }), OPTIONS);
    const unknown = reviewOf(proposed({ model: "ghost" }), OPTIONS);

    // THEN the first keeps it with where it runs, the second follows the default
    expect(offered.form.model).toEqual({ model_source: "server", model_id: "remote", preset_id: null });
    expect(unknown.form.model).toBeNull();
  });

  test("without a draft the form opens empty", () => {
    // GIVEN no draft
    // WHEN the form is opened
    const review = reviewOf(null, OPTIONS);

    // THEN nothing is filled and nothing can be saved yet
    expect(review).toMatchObject({ name: "", listed: [], checked: [], sources: [], notes: {} });
    expect(canSave(review.name, review.form)).toBe(false);
  });

  test("what the machine does not hold is named with what it is", () => {
    // GIVEN the entries a draft left out
    // WHEN they are read
    const views = unavailableViews(["mail_send", "source:src-ghost", "model:gpt-9"]);

    // THEN each says whether it is an access, a source or a model
    expect(views).toEqual([
      { kind: "access", name: "mail_send" },
      { kind: "source", name: "src-ghost" },
      { kind: "model", name: "gpt-9" },
    ]);
  });
});

describe("saveDraft - nothing is written before the form is saved", () => {
  test("reading the draft and opening the form reach no effect", () => {
    // GIVEN the effects of a save
    const effects = { create: vi.fn(), setModel: vi.fn() };

    // WHEN a checked sheet is read and opened in the form
    const draft = draftOfChecked(proposed());
    reviewOf(draft, OPTIONS);

    // THEN nothing was written
    expect(effects.create).not.toHaveBeenCalled();
    expect(effects.setModel).not.toHaveBeenCalled();
  });

  test("a form without a name or without a mission is not saved", async () => {
    // GIVEN a form whose mission was emptied, then one whose name was
    const effects = { create: vi.fn(), setModel: vi.fn() };
    const review = reviewOf(proposed(), OPTIONS);

    // WHEN each is saved
    const noMission = await saveDraft("Press watch", { ...review.form, mission: "  " }, effects);
    const noName = await saveDraft("  ", review.form, effects);

    // THEN neither reaches the runtime
    expect(noMission).toBeNull();
    expect(noName).toBeNull();
    expect(effects.create).not.toHaveBeenCalled();
  });

  test("saving sends the sheet as validated, with the accesses checked only", async () => {
    // GIVEN the form of a draft, where the person checked the sensitive access
    const effects = {
      create: vi.fn(async () => created()),
      setModel: vi.fn(async () => undefined),
    };
    const review = reviewOf(proposed(), OPTIONS);
    const form = { ...review.form, access: [...review.form.access, "mail_send"], examples: ["One", ""] };

    // WHEN the form is saved
    const outcome = await saveDraft(" Press watch ", form, effects);

    // THEN one agent is created from the sheet, and no model is assigned
    expect(effects.create).toHaveBeenCalledTimes(1);
    expect(effects.create).toHaveBeenCalledWith({
      name: "Press watch",
      mission: "Summarise the press every morning.",
      examples: ["One"],
      access: ["notes_lookup", "web_search", "mail_send"],
      sources: ["profile"],
    });
    expect(effects.setModel).not.toHaveBeenCalled();
    expect(outcome).toEqual({ agent: created(), modelError: null });
  });

  test("an unchecked sensitive access is not part of the sheet sent", () => {
    // GIVEN the form as it opens
    const review = reviewOf(proposed(), OPTIONS);

    // WHEN the sheet is built
    // THEN the sensitive access, left unchecked, is absent
    expect(sheetOf(review.name, review.form).access).toEqual(["notes_lookup", "web_search"]);
  });

  test("a chosen model is assigned to the agent once it exists", async () => {
    // GIVEN a form with a model chosen
    const effects = {
      create: vi.fn(async () => created()),
      setModel: vi.fn(async () => undefined),
    };
    const review = reviewOf(proposed({ model: "local" }), OPTIONS);

    // WHEN the form is saved
    await saveDraft(review.name, review.form, effects);

    // THEN the model is assigned to the identifier the runtime gave
    expect(effects.setModel).toHaveBeenCalledWith("press-watch", {
      model_source: "local",
      model_id: "local",
      preset_id: null,
    });
  });

  test("a model that cannot be assigned leaves the agent created and says why", async () => {
    // GIVEN a runtime that creates the agent and refuses the model
    const effects = {
      create: vi.fn(async () => created()),
      setModel: vi.fn(async () => {
        throw new Error("unknown model");
      }),
    };
    const review = reviewOf(proposed({ model: "local" }), OPTIONS);

    // WHEN the form is saved
    const outcome = await saveDraft(review.name, review.form, effects);

    // THEN the agent is answered with the reason
    expect(outcome).toEqual({ agent: created(), modelError: "unknown model" });
  });

  test("a creation the runtime refuses is reported, and no model is assigned", async () => {
    // GIVEN a runtime that refuses the sheet
    const effects = {
      create: vi.fn(async () => {
        throw new Error("audit journal not available");
      }),
      setModel: vi.fn(async () => undefined),
    };
    const review = reviewOf(proposed({ model: "local" }), OPTIONS);

    // WHEN the form is saved
    // THEN the refusal reaches the caller and nothing else is written
    await expect(saveDraft(review.name, review.form, effects)).rejects.toThrow("audit journal");
    expect(effects.setModel).not.toHaveBeenCalled();
  });
});
