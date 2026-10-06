import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { get } from "svelte/store";
import { describe, expect, test } from "vitest";
import { canContinueAiStep } from "./aiSetupRules";
import { ONBOARDING_STEPS } from "./onboardingSteps";
import {
  SERVER_FAILURE_KINDS,
  SERVER_FAILURE_TEXT,
  createServerSetup,
  isHttpAddress,
  serverModelRoute,
  serverNameFor,
  testFailureOf,
  type ServerSetupDeps,
} from "./serverSetup";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

// The form renders the state of one object, and vitest runs in `node`, so the
// object is driven here with a simulated IPC: every call the application
// would receive is recorded, and every answer is chosen by the test.

interface Call {
  command: "test" | "save" | "setDefault";
  args: unknown[];
}

function simulatedIpc(overrides: Partial<ServerSetupDeps> = {}) {
  const calls: Call[] = [];
  const deps: ServerSetupDeps = {
    test: async (...args) => {
      calls.push({ command: "test", args });
      return (overrides.test ?? (async () => ["llama3.2", "qwen2.5:14b"]))(...args);
    },
    save: async (...args) => {
      calls.push({ command: "save", args });
      return (overrides.save ?? (async () => ({})))(...args);
    },
    setDefault: async (...args) => {
      calls.push({ command: "setDefault", args });
      return (overrides.setDefault ?? (async () => ({})))(...args);
    },
  };
  return { calls, setup: createServerSetup(deps) };
}

function source(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

describe("first launch, use a server - the test lists the models", () => {
  test("a server that answers lists its models and saves nothing", async () => {
    // GIVEN a server that offers two models
    const { calls, setup } = simulatedIpc();

    // WHEN its address is tested, without a key
    await setup.test(" http://localhost:11434 ", "");

    // THEN the models are listed in the order of the server
    const state = get(setup);
    expect(state.phase).toBe("listed");
    expect(state.models).toEqual(["llama3.2", "qwen2.5:14b"]);
    expect(state.failure).toBeNull();
    // AND the only call was the test, on the trimmed address, with no key
    expect(calls).toEqual([{ command: "test", args: ["http://localhost:11434", null] }]);
    // AND nothing is chosen yet
    expect(state.chosen).toBeNull();
  });

  test("a key that was typed is sent with the test", async () => {
    // GIVEN a server that asks for a key
    const { calls, setup } = simulatedIpc();

    // WHEN the address is tested with one
    await setup.test("https://llm.example.org/v1", " sk-test ");

    // THEN the key goes with the call, without the spaces around it
    expect(calls[0].args).toEqual(["https://llm.example.org/v1", "sk-test"]);
  });

  test("an address that is not an http address is refused before any call", async () => {
    // GIVEN an address typed without its scheme
    const { calls, setup } = simulatedIpc();

    // WHEN it is tested
    await setup.test("localhost:11434", "");

    // THEN the form says the address is incomplete, and nothing was called
    expect(get(setup).failure).toEqual({ kind: "invalid_address", detail: null });
    expect(get(setup).phase).toBe("failed");
    expect(calls).toEqual([]);
  });

  test.each(["unreachable", "key_refused", "not_openai_compatible", "policy"] as const)(
    "a %s refusal is shown as its own readable error, with no model list",
    async (kind) => {
      // GIVEN a test the application refuses with that kind
      const refusal = { kind, message: `the runtime said: ${kind}` };
      const { setup } = simulatedIpc({ test: async () => Promise.reject(refusal) });

      // WHEN the address is tested
      await setup.test("http://localhost:11434", "");

      // THEN the failure keeps its kind and the sentence of the runtime
      const state = get(setup);
      expect(state.phase).toBe("failed");
      expect(state.failure).toEqual({ kind, detail: refusal.message });
      // AND no model is listed, so none can be chosen
      expect(state.models).toEqual([]);
      await setup.choose("llama3.2");
      expect(get(setup).chosen).toBeNull();
    },
  );

  test("a refusal of an unknown shape is still shown, under the general error", () => {
    // GIVEN rejections the application does not classify
    // WHEN each is read
    // THEN each lands on the general error and keeps what was said
    expect(testFailureOf("connection reset")).toEqual({ kind: "other", detail: "connection reset" });
    expect(testFailureOf(new Error("boom"))).toEqual({ kind: "other", detail: "boom" });
    expect(testFailureOf({ kind: "brand_new", message: "later" })).toEqual({
      kind: "other",
      detail: "later",
    });
    expect(testFailureOf(null)).toEqual({ kind: "other", detail: null });
  });

  test("a server with an empty list says so instead of showing an empty choice", async () => {
    // GIVEN a server that answers with no model
    const { setup } = simulatedIpc({ test: async () => [] });

    // WHEN it is tested
    await setup.test("http://localhost:11434", "");

    // THEN the form explains it
    expect(get(setup).phase).toBe("failed");
    expect(get(setup).failure?.kind).toBe("no_models");
  });

  test("changing the address drops what the previous address listed", async () => {
    // GIVEN a server tested and listed
    const { calls, setup } = simulatedIpc();
    await setup.test("http://localhost:11434", "");

    // WHEN the address is edited
    setup.edited();

    // THEN the list is gone, and a model of the old address cannot be chosen
    expect(get(setup).phase).toBe("idle");
    expect(get(setup).models).toEqual([]);
    await setup.choose("llama3.2");
    expect(calls.map((call) => call.command)).toEqual(["test"]);
  });

  test("an answer that arrives after the address changed is ignored", async () => {
    // GIVEN a test still waiting for its server
    let answer: (models: string[]) => void = () => {};
    const { setup } = simulatedIpc({
      test: () => new Promise<string[]>((resolve) => (answer = resolve)),
    });
    const pending = setup.test("http://old.example:8000", "");

    // WHEN the address is edited, then the old server answers
    setup.edited();
    answer(["stale-model"]);
    await pending;

    // THEN the models of the old address are not shown
    expect(get(setup).phase).toBe("idle");
    expect(get(setup).models).toEqual([]);
  });
});

describe("first launch, use a server - the chosen model becomes the default", () => {
  test("choosing a model saves the server, then makes the model the default", async () => {
    // GIVEN a server tested with a key
    const { calls, setup } = simulatedIpc();
    await setup.test("http://localhost:11434/v1", "sk-test");

    // WHEN one of its models is chosen
    await setup.choose("qwen2.5:14b");

    // THEN the server is saved under the name read from its address, with
    // the address and the key that were tested
    expect(calls[1]).toEqual({
      command: "save",
      args: ["localhost-11434", "http://localhost:11434/v1", "sk-test"],
    });
    // AND that model of that server becomes the default model, after the save
    expect(calls[2]).toEqual({
      command: "setDefault",
      args: ["server", "localhost-11434/qwen2.5:14b"],
    });
    expect(calls).toHaveLength(3);
    // AND the form names what was chosen
    expect(get(setup).chosen).toEqual({
      server: "localhost-11434",
      model: "qwen2.5:14b",
      baseUrl: "http://localhost:11434/v1",
    });
    expect(get(setup).choosing).toBeNull();
  });

  test("a model the server did not list cannot be chosen", async () => {
    // GIVEN a server tested and listed
    const { calls, setup } = simulatedIpc();
    await setup.test("http://localhost:11434", "");

    // WHEN a model outside its list is asked for
    await setup.choose("not-listed");

    // THEN nothing is saved
    expect(calls.map((call) => call.command)).toEqual(["test"]);
    expect(get(setup).chosen).toBeNull();
  });

  test("a save that fails is shown, chooses nothing and leaves the list", async () => {
    // GIVEN a server whose save is refused
    const { calls, setup } = simulatedIpc({
      save: async () => Promise.reject("secret store unavailable"),
    });
    await setup.test("http://localhost:11434", "");

    // WHEN a model is chosen
    await setup.choose("llama3.2");

    // THEN the failure is shown with the sentence of the runtime
    const state = get(setup);
    expect(state.failure).toEqual({ kind: "save", detail: "secret store unavailable" });
    // AND no default model was set, nothing is chosen
    expect(calls.map((call) => call.command)).toEqual(["test", "save"]);
    expect(state.chosen).toBeNull();
    // AND the list is still there to try again
    expect(state.phase).toBe("listed");
    expect(state.choosing).toBeNull();
  });

  test("a second choice replaces the first, on the same saved server", async () => {
    // GIVEN a model already chosen
    const { calls, setup } = simulatedIpc();
    await setup.test("http://localhost:11434", "");
    await setup.choose("llama3.2");

    // WHEN another model of the same server is chosen
    await setup.choose("qwen2.5:14b");

    // THEN the same server name is saved again and the default follows
    expect(calls[3].args[0]).toBe("localhost-11434");
    expect(calls[4].args).toEqual(["server", "localhost-11434/qwen2.5:14b"]);
    expect(get(setup).chosen?.model).toBe("qwen2.5:14b");
  });
});

describe("first launch, use a server - the name and the address", () => {
  test("the name of a server is read from its host and port", () => {
    // GIVEN addresses as people type them
    // WHEN each is turned into a server name
    // THEN the name holds only the characters a server name accepts
    expect(serverNameFor("http://localhost:11434")).toBe("localhost-11434");
    expect(serverNameFor("http://localhost:11434/v1/")).toBe("localhost-11434");
    expect(serverNameFor("HTTPS://LLM.Example.org")).toBe("llm-example-org");
    expect(serverNameFor("http://user:secret@10.0.0.4:8000/v1")).toBe("10-0-0-4-8000");
    expect(serverNameFor("http://[::1]:8080")).toBe("1-8080");
    for (const address of ["http://localhost:11434", "https://llm.example.org/v1?x=1"]) {
      expect(serverNameFor(address)).toMatch(/^[a-z0-9_-]+$/);
    }
    // AND an address with nothing usable still gets a name
    expect(serverNameFor("http://")).toBe("server");
    // AND a model of the server is addressed through that name
    expect(serverModelRoute("localhost-11434", "llama3.2")).toBe("localhost-11434/llama3.2");
  });

  test("only an http or https address with a host is one", () => {
    // GIVEN addresses, complete and not
    // WHEN each is checked
    // THEN the scheme and the host are both required
    expect(isHttpAddress("http://localhost:11434")).toBe(true);
    expect(isHttpAddress(" https://llm.example.org/v1 ")).toBe(true);
    expect(isHttpAddress("localhost:11434")).toBe(false);
    expect(isHttpAddress("ftp://example.org")).toBe(false);
    expect(isHttpAddress("http://")).toBe(false);
    expect(isHttpAddress("")).toBe(false);
  });
});

describe("first launch, use a server - the step it lives in", () => {
  const NOTHING = {
    probing: false,
    engineWired: false,
    backendCount: 0,
    serverChosen: false,
    downloadInFlight: false,
  };

  test("a chosen server opens continue, and a tested one does not", async () => {
    // GIVEN a machine with no model, and a server tested but not chosen
    const { setup } = simulatedIpc();
    await setup.test("http://localhost:11434", "");
    const before = canContinueAiStep({ ...NOTHING, serverChosen: get(setup).chosen !== null });

    // WHEN one of its models is chosen
    await setup.choose("llama3.2");
    const after = canContinueAiStep({ ...NOTHING, serverChosen: get(setup).chosen !== null });

    // THEN the only mandatory step is satisfied by the choice alone
    expect(before).toBe(false);
    expect(after).toBe(true);
  });

  test("the step reads that rule from the server setup", () => {
    // GIVEN the source of the step
    const shell = source("./OnboardingAiSetup.svelte");

    // WHEN what it gives the rule is read
    // THEN the chosen server is part of it
    expect(shell).toContain("serverChosen: $serverSetup.chosen !== null");
    // AND the form it renders drives that same setup
    expect(shell).toContain("<OnboardingServerForm setup={serverSetup} />");
  });

  test("after the AI step comes the embeddings model", () => {
    // GIVEN the declared sequence of the first launch, and the modal
    const ids = ONBOARDING_STEPS.map((step) => step.id);
    const modal = source("./OnboardingModal.svelte");
    const aiStep = modal.slice(
      modal.indexOf("<OnboardingAiSetup"),
      modal.indexOf("<OnboardingEmbeddingsStep"),
    );

    // WHEN the step that follows the AI one is read
    // THEN it is the embeddings step, in the sequence and in the modal
    expect(ids[ids.indexOf("ai-setup") + 1]).toBe("embeddings");
    expect(aiStep).toContain('goTo("embeddings")');
  });

  test("using a server leaves neither the step nor the first launch", () => {
    // GIVEN the sources of the step, of its two sections and of the modal
    const shell = source("./OnboardingAiSetup.svelte");
    const local = source("./OnboardingLlmSection.svelte");
    const form = source("./OnboardingServerForm.svelte");
    const modal = source("./OnboardingModal.svelte");
    const aiStep = modal.slice(
      modal.indexOf("<OnboardingAiSetup"),
      modal.indexOf("<OnboardingEmbeddingsStep"),
    );

    // WHEN what "use a server" does is read
    // THEN the choice only switches what the step shows
    expect(shell).toContain("onclick={() => (source = id)}");
    // AND nothing in the step navigates to another screen
    for (const file of [shell, local, form]) {
      expect(file).not.toMatch(/navigateTo|stores\/navigation|onopencloud|onclose/);
    }
    // AND the modal hands the step no way to close it
    expect(aiStep).not.toMatch(/onclose|onopencloud/);
    // AND the control that used to jump out is gone
    expect(local).not.toContain("onboarding-open-cloud");
  });

  test("every control of the form can be driven, and every error has its text", () => {
    // GIVEN the source of the form and the two catalogues
    const form = source("./OnboardingServerForm.svelte");

    // WHEN its controls are read
    // THEN the address, the key, the test, the list and the outcome are named
    for (const testid of [
      "onboarding-server-form",
      "onboarding-server-address",
      "onboarding-server-key",
      "onboarding-server-test",
      "onboarding-server-error",
      "onboarding-server-model",
      "onboarding-server-chosen",
    ]) {
      expect(form, testid).toContain(`data-testid="${testid}"`);
    }
    // AND each kind of failure has a title and says what to check, in both languages
    for (const catalogue of [fr, en]) {
      const errors = catalogue.onboarding.ai_setup.server.error as Record<string, string>;
      const prefix = "onboarding.ai_setup.server.error.";
      for (const kind of SERVER_FAILURE_KINDS) {
        const text = SERVER_FAILURE_TEXT[kind];
        expect(text.title).toBe(`${prefix}${kind}_title`);
        expect(text.body).toBe(`${prefix}${kind}_body`);
        expect(errors[text.title.slice(prefix.length)]?.trim(), text.title).toBeTruthy();
        expect(errors[text.body.slice(prefix.length)]?.trim(), text.body).toBeTruthy();
      }
      expect(Object.keys(errors)).toHaveLength(SERVER_FAILURE_KINDS.length * 2);
    }
  });
});
