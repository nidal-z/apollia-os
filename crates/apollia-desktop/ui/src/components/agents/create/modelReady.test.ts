import { describe, expect, test } from "vitest";
import { modelReadiness } from "./modelReady";

const server = { backend: "office-server", model_source: "server" as const, model_id: "office-server/qwen" };
const local = { backend: "local", model_source: "local" as const, model_id: "qwen.gguf" };

describe("the model the creation conversation speaks to", () => {
  test("a default served by a server that does not answer is named before any message", async () => {
    // GIVEN a default model on a declared server whose address refuses the connection
    const asked: string[] = [];
    // WHEN the creation checks the model
    const state = await modelReadiness({
      defaultModel: () => Promise.resolve(server),
      serverModels: (name) => {
        asked.push(name);
        return Promise.reject(new Error("unreachable"));
      },
    });
    // THEN the server is asked by its name, and the creation stops on it
    expect(asked).toEqual(["office-server"]);
    expect(state).toEqual({ ready: false, server: "office-server" });
  });

  test("a server that answers, a local model and no default all let the conversation start", async () => {
    // GIVEN the three cases where nothing is known to be down
    const refuses = (): Promise<unknown> => Promise.reject(new Error("never asked"));
    // WHEN each is checked
    const answering = await modelReadiness({
      defaultModel: () => Promise.resolve(server),
      serverModels: () => Promise.resolve({ models: [] }),
    });
    const onThisMachine = await modelReadiness({ defaultModel: () => Promise.resolve(local), serverModels: refuses });
    const none = await modelReadiness({ defaultModel: () => Promise.resolve(null), serverModels: refuses });
    const unread = await modelReadiness({
      defaultModel: () => Promise.reject(new Error("no runtime")),
      serverModels: refuses,
    });
    // THEN the conversation starts, and no server is asked for a local model
    expect([answering, onThisMachine, none, unread]).toEqual([
      { ready: true },
      { ready: true },
      { ready: true },
      { ready: true },
    ]);
  });
});
