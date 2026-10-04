import { describe, test, expect, vi, beforeEach } from "vitest";

const ipc = vi.hoisted(() => vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import type { ResolvedAgent } from "$lib/ipc/agentSheet";
import type { LlmBackendConfig } from "$lib/types";
import {
  adjustmentWrites,
  changedFields,
  chooseModel,
  draftFrom,
  filterCandidates,
  grant,
  grantCandidates,
  hasOverrides,
  isOverridden,
  listedAccesses,
  modelOptions,
  presetsOf,
  revoke,
  toggleSource,
} from "./agentAdjust";
import { createAgentAdjust } from "./useAgentAdjust.svelte";

function agent(over: Partial<ResolvedAgent> = {}): ResolvedAgent {
  return {
    id: "invoices",
    origin: "package",
    package: "billing",
    version: "1.2.0",
    enabled: true,
    name: "Invoices",
    mission: "Sort the invoices",
    examples: ["Sort this week's invoices"],
    access: ["file_read"],
    sources: ["profile"],
    model: null,
    autonomy: "supervised",
    profile: null,
    overridden: [],
    definition: {
      id: "invoices",
      mission: "Sort the invoices",
      examples: ["Sort this week's invoices"],
      defaults: { access: ["file_read"], sources: ["profile"] },
    },
    ...over,
  };
}

describe("overrides", () => {
  test("an unchanged draft writes nothing", () => {
    // GIVEN the draft of an agent, untouched
    const base = agent();
    const draft = draftFrom(base);

    // WHEN the writes are computed
    const writes = adjustmentWrites(base, draft);

    // THEN there is none
    expect(writes).toEqual([]);
  });

  test("each changed field becomes one override, in the order of the sheet", () => {
    // GIVEN an agent whose mission, examples, accesses and sources are changed
    const base = agent();
    const draft = draftFrom(base);
    draft.mission = "  Sort and file the invoices  ";
    draft.examples = ["File Monday's invoices", "  "];
    draft.access = grant(draft.access, "agent:veille");
    draft.sources = toggleSource(draft.sources, "profile", false);

    // WHEN the writes are computed
    const writes = adjustmentWrites(base, draft);

    // THEN one override per field, the values cleaned as they are saved
    expect(writes).toEqual([
      { kind: "set_override", field: "mission", value: "Sort and file the invoices" },
      { kind: "set_override", field: "examples", value: ["File Monday's invoices"] },
      { kind: "set_override", field: "access", value: ["file_read", "agent:veille"] },
      { kind: "set_override", field: "sources", value: [] },
    ]);
    expect([...changedFields(base, draft)]).toEqual(["mission", "examples", "access", "sources"]);
  });

  test("a field brought back to the package's value removes its override", () => {
    // GIVEN an agent whose mission is the user's
    const base = agent({ mission: "Mine", overridden: ["mission"] });
    const draft = draftFrom(base);

    // WHEN the draft writes the package's mission again
    draft.mission = "Sort the invoices";
    const writes = adjustmentWrites(base, draft);

    // THEN the override is removed rather than rewritten
    expect(writes).toEqual([{ kind: "remove_override", field: "mission" }]);
  });

  test("the order of the accesses alone is not a change", () => {
    // GIVEN an agent with two accesses
    const base = agent({ access: ["file_read", "mail_send"] });
    const draft = draftFrom(base);

    // WHEN the draft lists them the other way round
    draft.access = ["mail_send", "file_read"];

    // THEN nothing is written
    expect(adjustmentWrites(base, draft)).toEqual([]);
  });

  test("a model is assigned with its preset, then cleared back to the default", () => {
    // GIVEN an agent following the default model, then one with a model
    const following = agent();
    const assigned = agent({
      model: { agent: "invoices", model_source: "local", model_id: "local", preset_id: 4 },
    });
    const options = modelOptions([
      { name: "local", provider: "llama-cpp", model: "qwen3-8b", enabled: true, is_default: true, config_json: {} },
    ] as LlmBackendConfig[]);

    // WHEN the first is given a model and a preset, the second sent back to the default
    const toModel = draftFrom(following);
    toModel.model = chooseModel(options, toModel.model, "local");
    toModel.model = toModel.model ? { ...toModel.model, preset_id: 4 } : null;
    const toDefault = draftFrom(assigned);
    toDefault.model = chooseModel(options, toDefault.model, "");

    // THEN one assignment, then one removal
    expect(adjustmentWrites(following, toModel)).toEqual([
      { kind: "set_model", model: { model_source: "local", model_id: "local", preset_id: 4 } },
    ]);
    expect(adjustmentWrites(assigned, toDefault)).toEqual([{ kind: "clear_model" }]);
  });

  test("an overridden field and an assigned model are marked as the user's", () => {
    // GIVEN an agent with its access overridden and a model assigned
    const base = agent({
      overridden: ["access"],
      model: { agent: "invoices", model_source: "server", model_id: "vllm/qwen", preset_id: null },
    });

    // WHEN each field is asked whether it is the user's
    // THEN access and model are, mission is not
    expect(isOverridden(base, "access")).toBe(true);
    expect(isOverridden(base, "model")).toBe(true);
    expect(isOverridden(base, "mission")).toBe(false);
  });

  test("changing the model drops the preset of the previous one", () => {
    // GIVEN a draft on `local` with preset 4, and the presets of two models
    const options = modelOptions([
      { name: "local", provider: "llama-cpp", model: "a", enabled: true, is_default: true, config_json: {} },
      { name: "vllm", provider: "openai", model: "b", enabled: true, is_default: false, config_json: {} },
      { name: "off", provider: "openai", model: "c", enabled: false, is_default: false, config_json: {} },
    ] as LlmBackendConfig[]);
    const presets = [
      { id: 4, name: "Precise", model_source: "local" as const, model_id: "local", is_model_default: false },
      { id: 5, name: "Fast", model_source: "server" as const, model_id: "vllm", is_model_default: false },
    ];

    // WHEN the server model is chosen
    const next = chooseModel(options, { model_source: "local", model_id: "local", preset_id: 4 }, "vllm");

    // THEN it runs on a server, without preset, and only its own presets are offered
    expect(options.map((o) => o.id)).toEqual(["local", "vllm"]);
    expect(next).toEqual({ model_source: "server", model_id: "vllm", preset_id: null });
    expect(presetsOf(presets, next).map((p) => p.id)).toEqual([5]);
  });
});

describe("accesses granted with agent:", () => {
  test("agents and workflows are offered as agent: and workflow: accesses, never the agent itself", () => {
    // GIVEN two tools, two agents (one of them the agent adjusted) and a workflow
    const tools = [
      { name: "web_search", description: "Searches the web" },
      { name: "file_read", description: "Reads files" },
    ];
    const targets = [
      { kind: "agent" as const, item_id: "veille", name: "Veille", description: "Watches the web" },
      { kind: "agent" as const, item_id: "invoices", name: "Invoices", description: "Itself" },
      { kind: "workflow" as const, item_id: "billing/close-month", name: "Close month", description: "Closes" },
    ];

    // WHEN the candidates are listed with `file_read` and `agent:veille` granted
    const candidates = grantCandidates(tools, targets, ["file_read", "agent:veille"], "invoices");

    // THEN tools come first, then the agent and the workflow under their prefixes
    expect(candidates.map((c) => [c.id, c.granted])).toEqual([
      ["file_read", true],
      ["web_search", false],
      ["workflow:billing/close-month", false],
      ["agent:veille", true],
    ]);
    expect(filterCandidates(candidates, "watch").map((c) => c.id)).toEqual(["agent:veille"]);
  });

  test("an agent granted then revoked during the session stays listed, its switch off", () => {
    // GIVEN an agent with one access in force
    const inForce = ["file_read"];

    // WHEN `agent:veille` is granted, then `file_read` revoked
    const draft = revoke(grant(inForce, "agent:veille"), "file_read");

    // THEN both stay listed and only the agent is granted
    expect(listedAccesses(inForce, draft)).toEqual(["file_read", "agent:veille"]);
    expect(draft).toEqual(["agent:veille"]);
  });
});

describe("back to defaults", () => {
  beforeEach(() => {
    ipc.mockReset();
  });

  test("an agent with no override and no model has nothing to reset", () => {
    // GIVEN an agent on its package's values, then one with a model only
    // WHEN asked whether a return to defaults would remove anything
    // THEN only the second has something to remove
    expect(hasOverrides(agent())).toBe(false);
    expect(
      hasOverrides(agent({ model: { agent: "invoices", model_source: "local", model_id: "x", preset_id: null } })),
    ).toBe(true);
  });

  test("cancel drops the changes of the session and writes nothing", async () => {
    // GIVEN a session begun on an agent, its mission changed
    ipc.mockImplementation(async (cmd: string) => (cmd === "get_agent_definition" ? agent() : null));
    const session = createAgentAdjust(() => "invoices");
    await session.begin();
    if (session.draft) session.draft.mission = "Something else";
    expect(session.writes).toHaveLength(1);

    // WHEN the session is cancelled
    session.cancel();

    // THEN it ends and nothing but the read went to the runtime
    expect(session.active).toBe(false);
    expect(ipc.mock.calls.map((c) => c[0])).toEqual(["get_agent_definition"]);
  });

  test("save sends one write per changed field, then the return to defaults removes them all", async () => {
    // GIVEN a session whose mission and accesses change
    ipc.mockImplementation(async (cmd: string) => (cmd === "get_agent_definition" ? agent() : agent()));
    const session = createAgentAdjust(() => "invoices");
    await session.begin();
    if (session.draft) {
      session.draft.mission = "File the invoices";
      session.draft.access = grant(session.draft.access, "agent:veille");
    }

    // WHEN it is saved, then the agent is sent back to its defaults
    const saved = await session.save();
    const reset = await session.reset();

    // THEN the two overrides were written, then every override removed in one call
    expect(saved).toBe(true);
    expect(reset).toBe(true);
    expect(ipc.mock.calls.slice(1)).toEqual([
      ["set_agent_override", { name: "invoices", field: "mission", value: "File the invoices" }],
      ["set_agent_override", { name: "invoices", field: "access", value: ["file_read", "agent:veille"] }],
      ["reset_agent_overrides", { name: "invoices" }],
    ]);
    expect(session.revision).toBe(2);
  });
});
