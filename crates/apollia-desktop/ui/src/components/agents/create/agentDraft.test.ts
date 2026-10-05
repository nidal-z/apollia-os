import { describe, test, expect, vi } from "vitest";
import type { ResolvedAgent } from "$lib/ipc/agentSheet";
import type { ChatMessageView, ToolCallView } from "$lib/types";
import type { ModelOption } from "../sheet/agentAdjust";
import {
  AGENT_DRAFT_TOOL,
  DRAFT_SESSION_REQUEST,
  canSave,
  draftFields,
  extractDraft,
  reviewOf,
  saveDraft,
  sheetOf,
  unavailableViews,
  type ProposedAgent,
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

function call(
  output: unknown,
  status: ToolCallView["status"] = "executed",
  tool = AGENT_DRAFT_TOOL,
): ToolCallView {
  return {
    tool_name: tool,
    input: {},
    output: typeof output === "string" ? output : JSON.stringify(output),
    status,
  };
}

function message(seq: number, calls: ToolCallView[] | null, content = ""): ChatMessageView {
  return {
    id: `m${seq}`,
    role: "assistant",
    content,
    tool_calls: calls,
    tool_name: null,
    seq,
    created_at: "2026-10-05T00:00:00Z",
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

describe("the conversation that drafts an agent", () => {
  test("it is a conversation with Apollia whose tool list names the draft tool", () => {
    // GIVEN the request the Agents screen sends
    // WHEN it is read
    // THEN it asks for a free conversation offering the draft tool alone
    expect(DRAFT_SESSION_REQUEST).toEqual({ mode: "libre", tools: ["propose_agent"] });
  });
});

describe("extractDraft - the draft is read in the conversation", () => {
  test("a conversation where Apollia has not proposed holds no draft", () => {
    // GIVEN an exchange of plain messages
    const messages = [message(1, null, "Hello"), message(2, [], "What is it for?")];

    // WHEN the draft is looked for
    // THEN there is none
    expect(extractDraft(messages)).toBeNull();
  });

  test("the draft is the output of the tool call, with its six fields", () => {
    // GIVEN a message whose call answered with a draft
    const messages = [message(1, [call({ ok: true, draft: proposed() })], "Here is a draft.")];

    // WHEN the draft is read
    const draft = extractDraft(messages);

    // THEN it carries the structured sheet
    expect(draft).toEqual(proposed());
  });

  test("the latest draft wins over an earlier one", () => {
    // GIVEN two proposals in the same conversation
    const messages = [
      message(1, [call({ ok: true, draft: proposed({ name: "First" }) })]),
      message(2, null, "Call it otherwise"),
      message(3, [call({ ok: true, draft: proposed({ name: "Second" }) })]),
    ];

    // WHEN the draft is read
    // THEN it is the last one
    expect(extractDraft(messages)?.name).toBe("Second");
  });

  test("a call that failed, another tool and an unreadable output propose nothing", () => {
    // GIVEN a failed call, a call of another tool, and outputs that are no draft
    const messages = [
      message(1, [call({ ok: true, draft: proposed() }, "failed")]),
      message(2, [call({ ok: true, draft: proposed() }, "executed", "web_search")]),
      message(3, [call("not json")]),
      message(4, [call({ ok: false, error: "invalid payload" })]),
      message(5, [call({ ok: true, draft: { name: "No mission" } })]),
    ];

    // WHEN the draft is looked for
    // THEN there is none
    expect(extractDraft(messages)).toBeNull();
  });

  test("a sensitive access is never read as checked, whatever the answer says", () => {
    // GIVEN an answer that marks a sensitive access as checked
    const answer = {
      ok: true,
      draft: proposed({
        access: [{ id: "mail_send", checked: true, sensitive: true, by_default: false }],
      }),
    };

    // WHEN the draft is read
    const draft = extractDraft([message(1, [call(answer)])]);

    // THEN the access is listed, unchecked
    expect(draft?.access).toEqual([
      { id: "mail_send", checked: false, sensitive: true, by_default: false },
    ]);
  });
});

describe("draftFields - the draft beside the conversation", () => {
  test("before the first proposal every field is to come", () => {
    // GIVEN no draft and a conversation at rest
    // WHEN the fields are read
    const fields = draftFields(null, false);

    // THEN the six fields of a sheet are listed, none filled
    expect(fields.map((f) => f.field)).toEqual([
      "name",
      "mission",
      "examples",
      "model",
      "access",
      "sources",
    ]);
    expect(fields.every((f) => f.state === "upcoming")).toBe(true);
  });

  test("while Apollia answers, the first field still empty is being written", () => {
    // GIVEN no draft and Apollia answering
    // WHEN the fields are read
    const states = draftFields(null, true).map((f) => f.state);

    // THEN the name is being written and the rest is to come
    expect(states).toEqual(["writing", "upcoming", "upcoming", "upcoming", "upcoming", "upcoming"]);
  });

  test("a draft fills its fields, and a draft without a model follows the default", () => {
    // GIVEN a draft that names no model and no example
    const draft = proposed({ examples: [], model: null });

    // WHEN the fields are read at rest
    const fields = draftFields(draft, false);
    const byField = Object.fromEntries(fields.map((f) => [f.field, f]));

    // THEN every field it holds is filled, the model as the default one
    expect(byField.name).toMatchObject({ state: "filled", lines: ["Press watch"] });
    expect(byField.examples.state).toBe("upcoming");
    expect(byField.model).toMatchObject({ state: "filled", lines: [], defaultModel: true });
    expect(byField.access.lines).toEqual(["notes_lookup", "mail_send", "web_search"]);
    expect(byField.sources.lines).toEqual(["profile"]);
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

    // WHEN the draft is read, shown and opened in the form
    const draft = extractDraft([message(1, [call({ ok: true, draft: proposed() })])]);
    draftFields(draft, false);
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
