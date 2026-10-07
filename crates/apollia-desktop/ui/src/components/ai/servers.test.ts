import { describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";
import type {
  DeclaredServer,
  DefaultModel,
  ServerChanges,
  ServerModel,
} from "$lib/ipc/llmServers";
import {
  createServers,
  isDefaultModel,
  parsedExtraParams,
  parsedTimeout,
  seedServerForm,
  serverChanges,
  serverStatus,
  suggestedServerName,
  validateServerForm,
  type ServerFormState,
  type ServerModelsState,
  type ServersDeps,
} from "./servers";

function server(overrides: Partial<DeclaredServer> = {}): DeclaredServer {
  return {
    name: "ollama",
    base_url: "http://localhost:11434",
    has_key: false,
    api_key_env: null,
    enabled: true,
    timeout_secs: null,
    extra_params: null,
    ...overrides,
  };
}

function model(id: string, window: number | null = 32768, short = false): ServerModel {
  return { id, route: `ollama/${id}`, context_window: window, context_short: short };
}

function form(overrides: Partial<ServerFormState> = {}): ServerFormState {
  return {
    name: "ollama",
    address: "http://localhost:11434",
    apiKey: "",
    timeoutSecs: "",
    extraJson: "",
    ...overrides,
  };
}

/** A simulated application: the saved servers live in `saved`, as the runtime keeps them. */
function application(initial: DeclaredServer[] = [], models: ServerModel[] = [model("qwen2.5:14b")]) {
  const saved = new Map(initial.map((s) => [s.name, s]));
  let defaultModel: DefaultModel | null = null;
  const deps = {
    list: vi.fn(async () => [...saved.values()]),
    models: vi.fn(async (name: string) => ({
      server: name,
      models,
      expected_context_window: 32768,
    })),
    test: vi.fn(async (_baseUrl: string, _apiKey: string | null) => models.map((m) => m.id)),
    save: vi.fn(async (name: string, baseUrl: string, apiKey: string | null) => {
      const next = server({ name, base_url: baseUrl, has_key: apiKey !== null });
      saved.set(name, next);
      return next;
    }),
    update: vi.fn(async (name: string, changes: ServerChanges) => {
      const before = saved.get(name);
      if (!before) throw new Error(`server '${name}' not found`);
      const next: DeclaredServer = {
        ...before,
        base_url: changes.base_url ?? before.base_url,
        enabled: changes.enabled ?? before.enabled,
        has_key: changes.api_key === undefined ? before.has_key : changes.api_key !== null,
        timeout_secs:
          changes.timeout_secs === undefined ? before.timeout_secs : changes.timeout_secs,
        extra_params:
          changes.extra_params === undefined ? before.extra_params : changes.extra_params,
      };
      saved.set(name, next);
      return next;
    }),
    remove: vi.fn(async (name: string) => {
      saved.delete(name);
    }),
    getDefault: vi.fn(async () => defaultModel),
    setDefault: vi.fn(async (_source: "server", modelId: string) => {
      defaultModel = { backend: modelId, model_source: "server", model_id: modelId };
      return defaultModel;
    }),
    reloadRouter: vi.fn(async () => {}),
  } satisfies ServersDeps;
  return { deps, saved };
}

describe("the form of a server", () => {
  test("the key is optional for any server, a remote one included", () => {
    // GIVEN the form of a remote server and of a local one, both without a key
    const remote = form({ name: "corp", address: "https://llm.corp.example/v1" });
    const local = form();

    // WHEN each is validated, as a new server and as a change
    const verdicts = [remote, local].flatMap((state) => [
      validateServerForm(state, { editing: false, developer: false, takenNames: [] }),
      validateServerForm(state, { editing: true, developer: false, takenNames: [] }),
    ]);

    // THEN nothing is refused: no rule asks for a key
    expect(verdicts).toEqual([{}, {}, {}, {}]);
  });

  test("a name out of the pattern, a taken name and a bare host are refused", () => {
    // GIVEN three forms that cannot be saved
    const options = { editing: false, developer: false, takenNames: ["ollama"] };

    // WHEN each is validated
    const badName = validateServerForm(form({ name: "Ollama du bureau" }), options);
    const taken = validateServerForm(form({ name: "ollama" }), options);
    const bareHost = validateServerForm(form({ name: "corp", address: "10.0.0.5:8000" }), options);
    const empty = validateServerForm(form({ name: "", address: "" }), options);

    // THEN each names the field and the reason
    expect(badName).toEqual({ name: "invalid_name" });
    expect(taken).toEqual({ name: "name_taken" });
    expect(bareHost).toEqual({ address: "invalid_address" });
    expect(empty).toEqual({ name: "required", address: "required" });
  });

  test("the name of a new server follows its address until it is typed", () => {
    // GIVEN addresses, complete or not
    // WHEN the name is suggested
    // THEN a complete address gives a name the runtime accepts, an incomplete one none
    expect(suggestedServerName("http://localhost:11434/v1")).toBe("localhost-11434");
    expect(suggestedServerName("https://llm.corp.example")).toBe("llm-corp-example");
    expect(suggestedServerName("localhost")).toBe("");
  });

  test("the timeout and the extra parameters are read in developer mode only", () => {
    // GIVEN a form whose developer fields hold what cannot be stored
    const state = form({ timeoutSecs: "5", extraJson: "[1]" });

    // WHEN it is validated with the mode off, then on
    const off = validateServerForm(state, { editing: true, developer: false, takenNames: [] });
    const on = validateServerForm(state, { editing: true, developer: true, takenNames: [] });

    // THEN the mode off ignores fields it does not show, the mode on refuses both
    expect(off).toEqual({});
    expect(on).toEqual({ timeoutSecs: "invalid_timeout", extraJson: "invalid_json_object" });
  });

  test("a timeout is a whole number of seconds within its bounds, or nothing", () => {
    // GIVEN what can be typed in the field
    // WHEN it is read
    // THEN empty is the default, a value in bounds is itself, the rest is unreadable
    expect(parsedTimeout("")).toBeNull();
    expect(parsedTimeout(" 120 ")).toBe(120);
    expect(parsedTimeout("60")).toBe(60);
    expect(parsedTimeout("3600")).toBe(3600);
    expect(parsedTimeout("59")).toBeUndefined();
    expect(parsedTimeout("3601")).toBeUndefined();
    expect(parsedTimeout("1.5")).toBeUndefined();
    expect(parsedTimeout("soon")).toBeUndefined();
  });

  test("the extra parameters are a JSON object that names no reserved field", () => {
    // GIVEN what can be typed in the field
    // WHEN it is read
    // THEN an object is kept, an empty one is nothing, the rest says why
    expect(parsedExtraParams("")).toEqual({ ok: true, value: null });
    expect(parsedExtraParams("{}")).toEqual({ ok: true, value: null });
    expect(parsedExtraParams('{"keep_alive":"30m"}')).toEqual({
      ok: true,
      value: { keep_alive: "30m" },
    });
    expect(parsedExtraParams("{")).toEqual({ ok: false, error: "invalid_json" });
    expect(parsedExtraParams("null")).toEqual({ ok: false, error: "invalid_json_object" });
    expect(parsedExtraParams('{"model":"x"}')).toEqual({ ok: false, error: "reserved_field" });
  });

  test("the form of a saved server shows its settings and never its key", () => {
    // GIVEN a saved server with a key, a timeout and an extra parameter
    const saved = server({ has_key: true, timeout_secs: 120, extra_params: { keep_alive: "30m" } });

    // WHEN its form is seeded
    const seeded = seedServerForm(saved);

    // THEN the key field is empty, and the settings read as they were saved
    expect(seeded.apiKey).toBe("");
    expect(seeded.timeoutSecs).toBe("120");
    expect(JSON.parse(seeded.extraJson)).toEqual({ keep_alive: "30m" });
  });

  test("a change carries only what moved", () => {
    // GIVEN a saved server with a key and a timeout
    const saved = server({ has_key: true, timeout_secs: 120 });

    // WHEN the form is left alone, then its address changed, then a key typed
    const untouched = serverChanges(saved, seedServerForm(saved), true);
    const moved = serverChanges(saved, form({ address: "http://10.0.0.5:11434/", timeoutSecs: "120" }), true);
    const rekeyed = serverChanges(saved, form({ apiKey: " sk-new ", timeoutSecs: "120" }), true);
    const reset = serverChanges(saved, form({ timeoutSecs: "" }), true);
    const hidden = serverChanges(saved, form({ timeoutSecs: "" }), false);

    // THEN nothing, the address, the key, the timeout emptied; and a form that
    // does not show the timeout never empties it
    expect(untouched).toEqual({});
    expect(moved).toEqual({ base_url: "http://10.0.0.5:11434/" });
    expect(rekeyed).toEqual({ api_key: "sk-new" });
    expect(reset).toEqual({ timeout_secs: null });
    expect(hidden).toEqual({});
  });
});

describe("the status and the default mark of a server", () => {
  test("the status follows the switch, then the last answer of the models", () => {
    // GIVEN a server and each state of its models
    const failed = (kind: "unreachable" | "key_refused"): ServerModelsState => ({
      phase: "failed",
      models: [],
      expectedWindow: null,
      failure: { kind, detail: null },
    });

    // WHEN its status is read
    // THEN it says disabled first, then what the models last answered
    expect(serverStatus(server({ enabled: false }), undefined)).toBe("disabled");
    expect(serverStatus(server(), undefined)).toBe("checking");
    expect(serverStatus(server(), { phase: "loading", models: [], expectedWindow: null })).toBe("checking");
    expect(serverStatus(server(), { phase: "listed", models: [], expectedWindow: 32768 })).toBe("active");
    expect(serverStatus(server(), failed("unreachable"))).toBe("unreachable");
    expect(serverStatus(server(), failed("key_refused"))).toBe("failing");
  });

  test("only the model the machine defaults to carries the mark", () => {
    // GIVEN a default model on a server, and a local one
    const onServer: DefaultModel = {
      backend: "ollama/qwen2.5:14b",
      model_source: "server",
      model_id: "ollama/qwen2.5:14b",
    };
    const local: DefaultModel = { backend: "local", model_source: "local", model_id: "qwen.gguf" };

    // WHEN the rows of the server are marked
    // THEN the one model is the default, no other is, and none when the default is local
    expect(isDefaultModel(onServer, "ollama", "qwen2.5:14b")).toBe(true);
    expect(isDefaultModel(onServer, "ollama", "llama3.1:8b")).toBe(false);
    expect(isDefaultModel(onServer, "vllm", "qwen2.5:14b")).toBe(false);
    expect(isDefaultModel(local, "ollama", "qwen.gguf")).toBe(false);
    expect(isDefaultModel(null, "ollama", "qwen2.5:14b")).toBe(false);
  });
});

describe("the gestures of the servers section", () => {
  test("opening the section lists the servers and the models of the enabled ones", async () => {
    // GIVEN an enabled server, a disabled one, and a short-window model
    const { deps } = application(
      [server(), server({ name: "vllm", enabled: false })],
      [model("qwen2.5:14b"), model("mistral:7b", 4096, true)],
    );
    const servers = createServers(deps);

    // WHEN the section is refreshed
    await servers.refresh();

    // THEN both servers are listed, and only the enabled one was asked its models
    const state = get(servers);
    expect(state.loaded).toBe(true);
    expect(state.servers.map((s) => s.name)).toEqual(["ollama", "vllm"]);
    expect(deps.models).toHaveBeenCalledTimes(1);
    expect(deps.models).toHaveBeenCalledWith("ollama");
    const listed = state.models.ollama;
    expect(listed.phase).toBe("listed");
    expect(listed.expectedWindow).toBe(32768);
    expect(listed.models.find((m) => m.context_short)?.id).toBe("mistral:7b");
    expect(state.models.vllm).toBeUndefined();
  });

  test("Test, before saving, calls the test of the runtime and shows its models", async () => {
    // GIVEN a server that is not saved
    const { deps, saved } = application([], [model("qwen2.5:14b"), model("llama3.1:8b")]);
    const servers = createServers(deps);

    // WHEN the form is tested with a key typed with spaces around it
    const outcome = await servers.testForm(form({ apiKey: " sk-test " }), null);

    // THEN the runtime tested the address with the key, listed, and nothing was saved
    expect(deps.test).toHaveBeenCalledWith("http://localhost:11434", "sk-test");
    expect(outcome).toEqual({ phase: "listed", models: ["qwen2.5:14b", "llama3.1:8b"] });
    expect(deps.save).not.toHaveBeenCalled();
    expect(saved.size).toBe(0);
  });

  test("a refused test says what to check, in the kind the runtime gave", async () => {
    // GIVEN a runtime that cannot reach the server, and an address that is not one
    const { deps } = application();
    deps.test.mockRejectedValueOnce({ kind: "unreachable", message: "cannot be reached" });
    const servers = createServers(deps);

    // WHEN each is tested
    const unreachable = await servers.testForm(form(), null);
    const incomplete = await servers.testForm(form({ address: "localhost:11434" }), null);

    // THEN the refusal keeps its kind and its sentence; the incomplete address never left
    expect(unreachable).toEqual({
      phase: "failed",
      failure: { kind: "unreachable", detail: "cannot be reached" },
    });
    expect(incomplete).toEqual({
      phase: "failed",
      failure: { kind: "invalid_address", detail: null },
    });
    expect(deps.test).toHaveBeenCalledTimes(1);
  });

  test("testing a saved server left as it is reaches it with its saved key", async () => {
    // GIVEN a saved server with a key, whose form was not touched
    const saved = server({ has_key: true });
    const { deps } = application([saved]);
    const servers = createServers(deps);

    // WHEN its form is tested, then tested again with another address
    const untouched = await servers.testForm(seedServerForm(saved), saved);
    await servers.testForm(form({ address: "http://10.0.0.5:11434" }), saved);

    // THEN the first went through the saved server, the second through the typed address
    expect(untouched).toEqual({ phase: "listed", models: ["qwen2.5:14b"] });
    expect(deps.models).toHaveBeenCalledWith("ollama");
    expect(deps.test).toHaveBeenCalledTimes(1);
    expect(deps.test).toHaveBeenCalledWith("http://10.0.0.5:11434", null);
  });

  test("Save declares a new server without a key and lists its models", async () => {
    // GIVEN a form never tested, with no key
    const { deps } = application();
    const servers = createServers(deps);

    // WHEN it is saved
    const saved = await servers.saveForm(form({ name: "corp", address: " https://llm.corp.example/v1 " }), null, false);

    // THEN the server is saved with no key, listed, and its models were read
    expect(deps.save).toHaveBeenCalledWith("corp", "https://llm.corp.example/v1", null);
    expect(deps.update).not.toHaveBeenCalled();
    expect(saved.has_key).toBe(false);
    const state = get(servers);
    expect(state.servers.map((s) => s.name)).toEqual(["corp"]);
    expect(state.models.corp.phase).toBe("listed");
  });

  test("Save, developer mode on, stores the timeout and the extra parameters of a new server", async () => {
    // GIVEN the form of a new server with both developer settings
    const { deps } = application();
    const servers = createServers(deps);

    // WHEN it is saved with the mode on
    const saved = await servers.saveForm(
      form({ timeoutSecs: "120", extraJson: '{"keep_alive":"30m"}' }),
      null,
      true,
    );

    // THEN the server is declared, then given its settings
    expect(deps.update).toHaveBeenCalledWith("ollama", {
      timeout_secs: 120,
      extra_params: { keep_alive: "30m" },
    });
    expect(saved.timeout_secs).toBe(120);
    expect(saved.extra_params).toEqual({ keep_alive: "30m" });
  });

  test("Edit changes the address and the key of a saved server, and only them", async () => {
    // GIVEN a saved server
    const before = server({ has_key: true });
    const { deps } = application([before]);
    const servers = createServers(deps);
    await servers.refresh();

    // WHEN its address and its key are changed
    const after = await servers.saveForm(
      form({ address: "http://10.0.0.5:11434", apiKey: "sk-new" }),
      before,
      false,
    );

    // THEN one change carries both, and the list shows the new address
    expect(deps.update).toHaveBeenCalledWith("ollama", {
      base_url: "http://10.0.0.5:11434",
      api_key: "sk-new",
    });
    expect(deps.save).not.toHaveBeenCalled();
    expect(after.base_url).toBe("http://10.0.0.5:11434");
    expect(get(servers).servers[0].base_url).toBe("http://10.0.0.5:11434");
  });

  test("Edit, developer mode on, changes the timeout and the extra parameters", async () => {
    // GIVEN a saved server with a timeout
    const before = server({ timeout_secs: 120 });
    const { deps } = application([before]);
    const servers = createServers(deps);

    // WHEN the timeout is emptied and an extra parameter typed, the mode on
    await servers.saveForm(form({ timeoutSecs: "", extraJson: '{"num_ctx":8192}' }), before, true);

    // THEN the change empties the one and sets the other
    expect(deps.update).toHaveBeenCalledWith("ollama", {
      timeout_secs: null,
      extra_params: { num_ctx: 8192 },
    });
  });

  test("the switch disables a server, then enables it again and reads its models", async () => {
    // GIVEN an enabled server whose models are listed
    const { deps } = application([server()]);
    const servers = createServers(deps);
    await servers.refresh();

    // WHEN it is turned off, then on
    await servers.setEnabled("ollama", false);
    const off = get(servers).servers[0].enabled;
    const readsWhileOff = deps.models.mock.calls.length;
    await servers.setEnabled("ollama", true);

    // THEN each position reached the runtime, and turning it on read the models again
    expect(deps.update).toHaveBeenNthCalledWith(1, "ollama", { enabled: false });
    expect(deps.update).toHaveBeenNthCalledWith(2, "ollama", { enabled: true });
    expect(off).toBe(false);
    expect(get(servers).servers[0].enabled).toBe(true);
    expect(deps.models.mock.calls.length).toBe(readsWhileOff + 1);
    expect(get(servers).busy).toBeNull();
  });

  test("Use by default makes a model of the server the default model", async () => {
    // GIVEN a server with two models and no default
    const { deps } = application([server()], [model("qwen2.5:14b"), model("llama3.1:8b")]);
    const servers = createServers(deps);
    await servers.refresh();

    // WHEN the second model is made the default
    await servers.setDefault("ollama", "llama3.1:8b");

    // THEN the runtime received its route, the engines were told, and the mark moved
    expect(deps.setDefault).toHaveBeenCalledWith("server", "ollama/llama3.1:8b");
    expect(deps.reloadRouter).toHaveBeenCalledTimes(1);
    const state = get(servers);
    expect(isDefaultModel(state.defaultModel, "ollama", "llama3.1:8b")).toBe(true);
    expect(isDefaultModel(state.defaultModel, "ollama", "qwen2.5:14b")).toBe(false);
  });

  test("Delete forgets the server and its models", async () => {
    // GIVEN two saved servers
    const { deps } = application([server(), server({ name: "vllm" })]);
    const servers = createServers(deps);
    await servers.refresh();

    // WHEN one is deleted
    await servers.remove("vllm");

    // THEN the runtime forgot it, and neither it nor its models are listed
    expect(deps.remove).toHaveBeenCalledWith("vllm");
    const state = get(servers);
    expect(state.servers.map((s) => s.name)).toEqual(["ollama"]);
    expect(state.models.vllm).toBeUndefined();
  });

  test("a server that stops answering keeps its last list under the error, and Retry brings it back", async () => {
    // GIVEN a server whose models were listed
    const { deps } = application([server()]);
    const servers = createServers(deps);
    await servers.refresh();

    // WHEN it stops answering, then answers again on a retry
    deps.models.mockRejectedValueOnce({ kind: "unreachable", message: "cannot be reached" });
    await servers.loadModels("ollama");
    const down = get(servers).models.ollama;
    await servers.loadModels("ollama");
    const back = get(servers).models.ollama;

    // THEN down, it says unreachable and still shows what it offered; back, it lists
    expect(down.phase).toBe("failed");
    expect(down.models.map((m) => m.id)).toEqual(["qwen2.5:14b"]);
    expect(down.phase === "failed" && down.failure.kind).toBe("unreachable");
    expect(serverStatus(server(), down)).toBe("unreachable");
    expect(back.phase).toBe("listed");
  });

  test("a list of servers that cannot be read says so and keeps what was listed", async () => {
    // GIVEN a section already listing a server
    const { deps } = application([server()]);
    const servers = createServers(deps);
    await servers.refresh();

    // WHEN the runtime stops answering the list, then answers again
    deps.list.mockRejectedValueOnce(new Error("runtime unavailable"));
    await servers.refresh();
    const failed = get(servers);
    await servers.refresh();

    // THEN the error is said beside the servers already shown, and a retry clears it
    expect(failed.error).toBe("runtime unavailable");
    expect(failed.servers.map((s) => s.name)).toEqual(["ollama"]);
    expect(get(servers).error).toBeNull();
  });

  test("a refused gesture rejects with the words of the runtime and frees the section", async () => {
    // GIVEN a runtime that refuses the change
    const { deps } = application([server()]);
    deps.update.mockRejectedValueOnce(new Error("secret store unavailable"));
    const servers = createServers(deps);
    await servers.refresh();

    // WHEN the switch is played
    const attempt = servers.setEnabled("ollama", false);

    // THEN the refusal reaches the caller, and the next gesture is not blocked
    await expect(attempt).rejects.toThrow("secret store unavailable");
    expect(get(servers).busy).toBeNull();
    expect(get(servers).servers[0].enabled).toBe(true);
  });
});
