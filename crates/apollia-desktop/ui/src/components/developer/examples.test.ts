import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { LocalServerView } from "$lib/ipc/localServer";
import {
  SERVER_SETTING_DEFAULTS,
  SERVER_SETTING_KEYS,
  type ServerSetting,
  type ServerSettingKey,
} from "$lib/stores/serverSettings";
import {
  EXAMPLE_IDS,
  EXAMPLE_LABELS,
  EXAMPLE_MCP_URL,
  FIRST_EXAMPLE,
  NO_TOKEN,
  REFUSAL_REASONS,
  SDK_LABELS,
  chosenSdk,
  exampleEntries,
  exampleFor,
  exampleRefusal,
  exampleSdks,
  exampleServer,
  exampleSlug,
  exampleText,
  exposedTarget,
  presentsToken,
  type ExampleAsk,
  type ExampleId,
  type ExampleSdk,
  type ExampleServer,
  type ExampleTarget,
  type ExampleTexts,
} from "./examples";

const HERE = path.dirname(fileURLToPath(import.meta.url));

function source(relative: string): string {
  return readFileSync(path.resolve(HERE, relative), "utf8");
}

const TEXTS: ExampleTexts = {
  prompt: "Hello",
  toolPrompt: "What is the weather in Paris?",
  mcpPrompt: "Which tools can you call?",
};

const LLM: ExampleTarget = { kind: "llm", identifier: "qwen/qwen3-8b" };
const EMBEDDINGS: ExampleTarget = { kind: "embeddings", identifier: "nomic-ai/nomic-embed" };
const AGENT: ExampleTarget = { kind: "agent", identifier: "apollia/demand-clerk" };
const WORKFLOW: ExampleTarget = {
  kind: "workflow",
  identifier: "apollia/office/close-month",
  fields: ["client", "month"],
};

const ADDRESS = "http://127.0.0.1:7772";

function server(over: Partial<ExampleServer> = {}): ExampleServer {
  return {
    address: ADDRESS,
    requireAuth: false,
    serveOnLan: false,
    allowRequestMcp: true,
    ...over,
  };
}

function ask(over: Partial<ExampleAsk> = {}): ExampleAsk {
  return { id: "curl_simple", target: LLM, address: ADDRESS, token: false, sdk: "openai", texts: TEXTS, ...over };
}

function settings(
  over: Partial<Record<ServerSettingKey, boolean | number>> = {},
): Record<ServerSettingKey, ServerSetting> {
  const all = { ...SERVER_SETTING_DEFAULTS, ...over };
  const entries = Object.entries(all).map(([key, value]) => [key, { value, source: "default" }]);
  return Object.fromEntries(entries) as Record<ServerSettingKey, ServerSetting>;
}

function view(over: Partial<LocalServerView> = {}): LocalServerView {
  return {
    running: true,
    port: 7772,
    serveOnLan: false,
    address: ADDRESS,
    lastError: null,
    activeTokens: 0,
    imposedByPolicy: { serveOnLan: false, requireAuth: false, allowRequestMcp: false },
    ...over,
  };
}

/** What a shell makes of a cURL example: its address, its headers, its body. */
function readCurl(text: string): { url: string; headers: string[]; body: unknown } {
  const joined = text.replace(/ \\\n\s*/g, " ");
  const url = /^curl (\S+)/.exec(joined)?.[1] ?? "";
  const headers = [...joined.matchAll(/-H "([^"]*)"/g)].map((match) => match[1] ?? "");
  const start = joined.indexOf("-d '");
  const quoted = joined.slice(start + 4, joined.lastIndexOf("'"));
  return { url, headers, body: JSON.parse(quoted.replace(/'\\''/g, "'")) };
}

describe("the simple cURL example", () => {
  test("a chat model is called on chat completions, with its identifier and a user message", () => {
    // GIVEN a chat model on a server that asks for no token
    // WHEN the simple example is written
    const call = readCurl(exampleText(ask()));
    // THEN it posts the identifier and one user message to chat completions
    expect(call.url).toBe("http://127.0.0.1:7772/v1/chat/completions");
    expect(call.headers).toEqual(["Content-Type: application/json"]);
    expect(call.body).toEqual({
      model: "qwen/qwen3-8b",
      messages: [{ role: "user", content: "Hello" }],
    });
  });

  test("a call that presents a token names the variable, never a value", () => {
    // GIVEN a call that must present a token
    // WHEN the simple example is written
    const text = exampleText(ask({ token: true }));
    // THEN the bearer is the variable, inside double quotes so the shell expands it
    expect(readCurl(text).headers).toContain("Authorization: Bearer $APOLLIA_TOKEN");
    expect(text).toContain('-H "Authorization: Bearer $APOLLIA_TOKEN"');
    expect(text).not.toMatch(/apo_/);
  });

  test("an embeddings model is called on embeddings, with an input", () => {
    // GIVEN an embeddings model
    // WHEN the simple example is written
    const call = readCurl(exampleText(ask({ target: EMBEDDINGS })));
    // THEN it posts an input to the embeddings endpoint, which is all that model serves
    expect(call.url).toBe("http://127.0.0.1:7772/v1/embeddings");
    expect(call.body).toEqual({ model: "nomic-ai/nomic-embed", input: "Hello" });
  });

  test("an agent is called like a model, by its apollia identifier", () => {
    // GIVEN an exposed agent
    // WHEN the simple example is written
    const call = readCurl(exampleText(ask({ target: AGENT })));
    // THEN the model field carries apollia/<id> and the message is the request
    expect(call.url).toBe("http://127.0.0.1:7772/v1/chat/completions");
    expect(call.body).toEqual({
      model: "apollia/demand-clerk",
      messages: [{ role: "user", content: "Hello" }],
    });
  });

  test("a workflow receives its inputs as a JSON object in the user message", () => {
    // GIVEN a workflow with two inputs
    // WHEN the simple example is written
    const call = readCurl(exampleText(ask({ target: WORKFLOW }))) as {
      body: { model: string; messages: { content: string }[] };
    };
    // THEN the user message is a JSON object holding each input, in the declared order
    expect(call.body.model).toBe("apollia/office/close-month");
    expect(JSON.parse(call.body.messages[0]?.content ?? "")).toEqual({ client: "...", month: "..." });
    expect(Object.keys(JSON.parse(call.body.messages[0]?.content ?? ""))).toEqual(["client", "month"]);
  });

  test("a workflow whose inputs are not known receives an empty object", () => {
    // GIVEN a workflow whose inputs could not be read
    const target = exposedTarget("workflow", "apollia/office/close-month", { kind: "unknown" });
    // WHEN the simple example is written
    const call = readCurl(exampleText(ask({ target }))) as { body: { messages: { content: string }[] } };
    // THEN the user message is an empty JSON object, which the reader fills
    expect(call.body.messages[0]?.content).toBe("{}");
  });

  test("an identifier holding a quote stays one shell argument", () => {
    // GIVEN an identifier with a single quote and a double quote
    const target: ExampleTarget = { kind: "llm", identifier: `o'neil/"big"` };
    // WHEN the simple example is written
    const text = exampleText(ask({ target }));
    // THEN the single quote is closed, escaped and reopened, and the body reads back whole
    expect(text).toContain(`o'\\''neil`);
    expect((readCurl(text).body as { model: string }).model).toBe(`o'neil/"big"`);
  });
});

describe("the cURL example with an MCP server provided by the request", () => {
  test("it posts an mcp tool to the responses endpoint", () => {
    // GIVEN a chat model
    // WHEN the MCP example is written
    const call = readCurl(exampleText(ask({ id: "curl_mcp" })));
    // THEN the server is an entry of tools whose type is mcp, on /v1/responses
    expect(call.url).toBe("http://127.0.0.1:7772/v1/responses");
    expect(call.body).toEqual({
      model: "qwen/qwen3-8b",
      input: "Which tools can you call?",
      tools: [{ type: "mcp", server_label: "example", server_url: EXAMPLE_MCP_URL }],
    });
  });

  test("with a token to present, it names the variable", () => {
    // GIVEN a call that must present a token
    // WHEN the MCP example is written
    const call = readCurl(exampleText(ask({ id: "curl_mcp", token: true })));
    // THEN the bearer is the variable
    expect(call.headers).toEqual(["Content-Type: application/json", "Authorization: Bearer $APOLLIA_TOKEN"]);
  });

  test("it is greyed out while the setting is off, with the way to turn it on", () => {
    // GIVEN a server whose setting for request MCP servers is off
    const off = server({ allowRequestMcp: false });
    // WHEN the menu of a model is listed
    const entry = exampleEntries(LLM, off).find((item) => item.id === "curl_mcp");
    // THEN the entry stays, with its reason, and has no text to copy
    expect(exampleRefusal("curl_mcp", LLM, off)).toBe("mcp_off");
    expect(entry?.reason).toBe(REFUSAL_REASONS.mcp_off);
    expect(exampleFor("curl_mcp", LLM, off, "openai", TEXTS)).toBeNull();
  });

  test("it is offered once the setting is on", () => {
    // GIVEN a server whose setting is on
    // WHEN the menu of a model is listed
    const entry = exampleEntries(LLM, server()).find((item) => item.id === "curl_mcp");
    // THEN the entry can be copied
    expect(entry?.reason).toBeNull();
    expect(exampleFor("curl_mcp", LLM, server(), "openai", TEXTS)).toContain("/v1/responses");
  });

  test("an agent or a workflow never has it, setting on or off", () => {
    // GIVEN an agent and a workflow, which the server refuses an MCP server on
    for (const target of [AGENT, WORKFLOW]) {
      for (const allowRequestMcp of [true, false]) {
        // WHEN the entry is judged
        const refusal = exampleRefusal("curl_mcp", target, server({ allowRequestMcp }));
        // THEN it is reserved to models
        expect(refusal).toBe("models_only");
      }
    }
  });
});

describe("the cURL example with a tool call", () => {
  test("it posts a function tool beside the user message", () => {
    // GIVEN a chat model
    // WHEN the tool example is written
    const call = readCurl(exampleText(ask({ id: "curl_tool" }))) as {
      url: string;
      body: { messages: unknown; tools: { type: string; function: { name: string; parameters: unknown } }[] };
    };
    // THEN chat completions receives one function tool and a question it answers
    expect(call.url).toBe("http://127.0.0.1:7772/v1/chat/completions");
    expect(call.body.messages).toEqual([{ role: "user", content: "What is the weather in Paris?" }]);
    expect(call.body.tools).toHaveLength(1);
    expect(call.body.tools[0]?.type).toBe("function");
    expect(call.body.tools[0]?.function.name).toBe("get_weather");
    expect(call.body.tools[0]?.function.parameters).toEqual({
      type: "object",
      properties: { city: { type: "string" } },
      required: ["city"],
    });
  });

  test("with a token to present, it names the variable", () => {
    // GIVEN a call that must present a token
    // WHEN the tool example is written
    const call = readCurl(exampleText(ask({ id: "curl_tool", token: true })));
    // THEN the bearer is the variable
    expect(call.headers).toContain("Authorization: Bearer $APOLLIA_TOKEN");
  });

  test("it is for models only: an agent and a workflow refuse tools", () => {
    // GIVEN an agent and a workflow
    for (const target of [AGENT, WORKFLOW]) {
      // WHEN the menu is listed
      const entry = exampleEntries(target, server()).find((item) => item.id === "curl_tool");
      // THEN the entry stays, greyed, with the reason, and has no text
      expect(entry?.reason).toBe(REFUSAL_REASONS.models_only);
      expect(exampleFor("curl_tool", target, server(), "openai", TEXTS)).toBeNull();
    }
  });

  test("an embeddings model has neither the tool nor the MCP example", () => {
    // GIVEN an embeddings model, which holds no conversation
    // WHEN both entries are judged
    // THEN each is reserved to chat models
    expect(exampleRefusal("curl_tool", EMBEDDINGS, server())).toBe("chat_only");
    expect(exampleRefusal("curl_mcp", EMBEDDINGS, server())).toBe("chat_only");
  });
});

describe("the Python example", () => {
  test("the OpenAI SDK is pointed at the address under /v1", () => {
    // GIVEN a chat model and no token to present
    // WHEN the Python example is written with the OpenAI SDK
    const text = exampleText(ask({ id: "python" }));
    // THEN the official client reads the address and a placeholder key
    expect(text).toContain("from openai import OpenAI");
    expect(text).toContain('base_url="http://127.0.0.1:7772/v1"');
    expect(text).toContain(`api_key="${NO_TOKEN}"`);
    expect(text).toContain('model="qwen/qwen3-8b"');
    expect(text).toContain('messages=[{"role": "user", "content": "Hello"}]');
    expect(text).toContain("client.chat.completions.create(");
    expect(text).not.toContain("import os");
  });

  test("the OpenAI SDK reads the token from the environment", () => {
    // GIVEN a call that must present a token
    // WHEN the Python example is written with the OpenAI SDK
    const text = exampleText(ask({ id: "python", token: true }));
    // THEN the key is the variable of the environment, never a value
    expect(text.startsWith("import os\n")).toBe(true);
    expect(text).toContain('api_key=os.environ["APOLLIA_TOKEN"]');
    expect(text).not.toContain(NO_TOKEN);
    expect(text).not.toMatch(/apo_/);
  });

  test("the Anthropic SDK is pointed at the address itself and calls messages", () => {
    // GIVEN a chat model and no token to present
    // WHEN the Python example is written with the Anthropic SDK
    const text = exampleText(ask({ id: "python", sdk: "anthropic" }));
    // THEN the official client reads the address without /v1, and sends max_tokens
    expect(text).toContain("from anthropic import Anthropic");
    expect(text).toContain('base_url="http://127.0.0.1:7772",');
    expect(text).toContain(`api_key="${NO_TOKEN}"`);
    expect(text).toContain("client.messages.create(");
    expect(text).toContain("max_tokens=1024,");
    expect(text).toContain("print(message.content[-1].text)");
  });

  test("the Anthropic SDK reads the token from the environment", () => {
    // GIVEN a call that must present a token
    // WHEN the Python example is written with the Anthropic SDK
    const text = exampleText(ask({ id: "python", sdk: "anthropic", token: true }));
    // THEN the key is the variable of the environment
    expect(text).toContain('api_key=os.environ["APOLLIA_TOKEN"]');
    expect(text).not.toContain(NO_TOKEN);
  });

  test("an embeddings model is called through embeddings", () => {
    // GIVEN an embeddings model
    // WHEN the Python example is written
    const text = exampleText(ask({ id: "python", target: EMBEDDINGS }));
    // THEN the OpenAI client creates an embedding from an input
    expect(text).toContain("client.embeddings.create(");
    expect(text).toContain('input="Hello"');
    expect(text).not.toContain("chat.completions");
  });

  test("an agent and a workflow are called like a model", () => {
    // GIVEN an agent and a workflow
    // WHEN the Python example of each is written
    const agent = exampleText(ask({ id: "python", target: AGENT }));
    const workflow = exampleText(ask({ id: "python", target: WORKFLOW, sdk: "anthropic" }));
    // THEN the identifier is the model, and the workflow receives its inputs as JSON text
    expect(agent).toContain('model="apollia/demand-clerk"');
    expect(workflow).toContain('model="apollia/office/close-month"');
    expect(workflow).toContain(String.raw`"content": "{\"client\":\"...\",\"month\":\"...\"}"`);
  });
});

describe("the JavaScript example", () => {
  test("the OpenAI SDK is pointed at the address under /v1", () => {
    // GIVEN a chat model and no token to present
    // WHEN the JavaScript example is written with the OpenAI SDK
    const text = exampleText(ask({ id: "javascript" }));
    // THEN the official client reads the address and a placeholder key
    expect(text).toContain('import OpenAI from "openai";');
    expect(text).toContain('baseURL: "http://127.0.0.1:7772/v1"');
    expect(text).toContain(`apiKey: "${NO_TOKEN}"`);
    expect(text).toContain('model: "qwen/qwen3-8b"');
    expect(text).toContain("client.chat.completions.create({");
  });

  test("the OpenAI SDK reads the token from the environment", () => {
    // GIVEN a call that must present a token
    // WHEN the JavaScript example is written with the OpenAI SDK
    const text = exampleText(ask({ id: "javascript", token: true }));
    // THEN the key is the variable of the environment, never a value
    expect(text).toContain("apiKey: process.env.APOLLIA_TOKEN,");
    expect(text).not.toContain(NO_TOKEN);
    expect(text).not.toMatch(/apo_/);
  });

  test("the Anthropic SDK is pointed at the address itself and calls messages", () => {
    // GIVEN a chat model and no token to present
    // WHEN the JavaScript example is written with the Anthropic SDK
    const text = exampleText(ask({ id: "javascript", sdk: "anthropic" }));
    // THEN the official client reads the address without /v1, and sends max_tokens
    expect(text).toContain('import Anthropic from "@anthropic-ai/sdk";');
    expect(text).toContain('baseURL: "http://127.0.0.1:7772",');
    expect(text).toContain("client.messages.create({");
    expect(text).toContain("max_tokens: 1024,");
  });

  test("the Anthropic SDK reads the token from the environment", () => {
    // GIVEN a call that must present a token
    // WHEN the JavaScript example is written with the Anthropic SDK
    const text = exampleText(ask({ id: "javascript", sdk: "anthropic", token: true }));
    // THEN the key is the variable of the environment
    expect(text).toContain("apiKey: process.env.APOLLIA_TOKEN,");
  });

  test("an embeddings model is called through embeddings", () => {
    // GIVEN an embeddings model
    // WHEN the JavaScript example is written
    const text = exampleText(ask({ id: "javascript", target: EMBEDDINGS }));
    // THEN the OpenAI client creates an embedding from an input
    expect(text).toContain("client.embeddings.create({");
    expect(text).toContain('input: "Hello"');
  });

  test("an agent is called like a model", () => {
    // GIVEN an exposed agent
    // WHEN the JavaScript example is written
    const text = exampleText(ask({ id: "javascript", target: AGENT }));
    // THEN the identifier is the model
    expect(text).toContain('model: "apollia/demand-clerk"');
  });
});

describe("an example is a function of the identifier, the address and the token", () => {
  const ids: ExampleId[] = [...EXAMPLE_IDS];
  const sdks: ExampleSdk[] = ["openai", "anthropic"];

  test("the same ask gives the same text", () => {
    // GIVEN each example, each SDK, with and without a token
    for (const id of ids) {
      for (const sdk of sdks) {
        for (const token of [true, false]) {
          // WHEN it is written twice
          const once = exampleText(ask({ id, sdk, token }));
          const twice = exampleText(ask({ id, sdk, token }));
          // THEN the two texts are the same
          expect(twice).toBe(once);
        }
      }
    }
  });

  test("every example carries the address and the identifier it was given", () => {
    // GIVEN another address and another identifier
    const target: ExampleTarget = { kind: "llm", identifier: "acme/chat" };
    for (const id of ids) {
      for (const sdk of sdks) {
        // WHEN the example is written
        const text = exampleText(ask({ id, sdk, target, address: "http://192.168.1.20:9000" }));
        // THEN it names them, and nothing of the defaults
        expect(text).toContain("http://192.168.1.20:9000");
        expect(text).toContain("acme/chat");
        expect(text).not.toContain("127.0.0.1");
      }
    }
  });

  test("a token is the variable in every example, and absent when none is presented", () => {
    for (const id of ids) {
      for (const sdk of sdks) {
        // GIVEN an example with a token to present, and the same without
        // WHEN both are written
        const withToken = exampleText(ask({ id, sdk, token: true }));
        const without = exampleText(ask({ id, sdk, token: false }));
        // THEN only the first names the variable, and neither holds a value
        expect(withToken).toContain("APOLLIA_TOKEN");
        expect(without).not.toContain("APOLLIA_TOKEN");
        expect(withToken).not.toMatch(/apo_[0-9a-f]/);
      }
    }
  });
});

describe("when a call presents a token", () => {
  test("every call does once the server asks for one", () => {
    // GIVEN a server that requires a token
    const strict = server({ requireAuth: true });
    // WHEN each example of each target is judged
    // THEN each presents the token
    for (const target of [LLM, EMBEDDINGS, AGENT, WORKFLOW]) {
      for (const id of EXAMPLE_IDS) expect(presentsToken(id, target, strict)).toBe(true);
    }
  });

  test("none does on this machine when the server asks for none", () => {
    // GIVEN a server on the loopback that requires no token
    // WHEN each example is judged
    // THEN none presents a token
    for (const target of [LLM, AGENT, WORKFLOW]) {
      for (const id of EXAMPLE_IDS) expect(presentsToken(id, target, server())).toBe(false);
    }
  });

  test("on the local network an agent, a workflow and a request MCP server do, a model does not", () => {
    // GIVEN a server on the local network that requires no token
    const lan = server({ serveOnLan: true, address: "http://192.168.1.20:7772" });
    // WHEN the examples are judged
    // THEN what never answers the network without a token presents one
    expect(presentsToken("curl_simple", AGENT, lan)).toBe(true);
    expect(presentsToken("python", WORKFLOW, lan)).toBe(true);
    expect(presentsToken("curl_mcp", LLM, lan)).toBe(true);
    expect(presentsToken("curl_simple", LLM, lan)).toBe(false);
    expect(presentsToken("curl_tool", LLM, lan)).toBe(false);
    expect(exampleFor("curl_simple", AGENT, lan, "openai", TEXTS)).toContain("$APOLLIA_TOKEN");
    expect(exampleFor("curl_simple", LLM, lan, "openai", TEXTS)).not.toContain("APOLLIA_TOKEN");
  });
});

describe("the menu of a card", () => {
  test("a model has the five entries, in reading order", () => {
    // GIVEN a chat model and the MCP setting on
    // WHEN the menu is listed
    const entries = exampleEntries(LLM, server());
    // THEN the five entries can all be copied
    expect(entries.map((entry) => entry.id)).toEqual([
      "curl_simple",
      "curl_mcp",
      "curl_tool",
      "python",
      "javascript",
    ]);
    expect(entries.every((entry) => entry.reason === null)).toBe(true);
    expect(entries.map((entry) => entry.label)).toEqual(EXAMPLE_IDS.map((id) => EXAMPLE_LABELS[id]));
  });

  test("an agent keeps the five entries, two of them greyed", () => {
    // GIVEN an exposed agent
    // WHEN the menu is listed
    const entries = exampleEntries(AGENT, server());
    // THEN the tool and MCP entries are greyed, the three others can be copied
    expect(entries).toHaveLength(5);
    expect(entries.filter((entry) => entry.reason !== null).map((entry) => entry.id)).toEqual([
      "curl_mcp",
      "curl_tool",
    ]);
  });

  test("Python and JavaScript offer both SDKs, cURL none, embeddings the OpenAI one alone", () => {
    // GIVEN the entries of a chat model and of an embeddings model
    // WHEN the SDKs of each are listed
    // THEN the choice exists where two SDKs can make the call
    expect(exampleSdks("python", LLM)).toEqual(["openai", "anthropic"]);
    expect(exampleSdks("javascript", AGENT)).toEqual(["openai", "anthropic"]);
    expect(exampleSdks("curl_simple", LLM)).toEqual([]);
    expect(exampleSdks("python", EMBEDDINGS)).toEqual(["openai"]);
    expect(chosenSdk("python", EMBEDDINGS, "anthropic")).toBe("openai");
    expect(chosenSdk("python", LLM, "anthropic")).toBe("anthropic");
    expect(exampleFor("python", EMBEDDINGS, server(), "anthropic", TEXTS)).toContain("from openai import OpenAI");
  });

  test("the preview opens on an entry every card can use", () => {
    // GIVEN each kind of card, the MCP setting off
    const off = server({ allowRequestMcp: false });
    // WHEN the first entry is written
    // THEN it has a text for all of them
    for (const target of [LLM, EMBEDDINGS, AGENT, WORKFLOW]) {
      expect(exampleFor(FIRST_EXAMPLE, target, off, "openai", TEXTS)).not.toBeNull();
    }
  });

  test("the test ids of the entries are stable", () => {
    // GIVEN the entries
    // WHEN the end of their test ids is built
    // THEN it carries no underscore and no name
    expect(EXAMPLE_IDS.map(exampleSlug)).toEqual([
      "curl-simple",
      "curl-mcp",
      "curl-tool",
      "python",
      "javascript",
    ]);
  });

  test("an exposed item becomes the target of its examples", () => {
    // GIVEN an agent and a workflow as the list of exposed items gives them
    // WHEN their targets are built
    // THEN the workflow carries its fields, the agent none
    expect(exposedTarget("agent", "apollia/a", { kind: "message" })).toEqual({
      kind: "agent",
      identifier: "apollia/a",
    });
    expect(exposedTarget("workflow", "apollia/p/w", { kind: "fields", names: ["x"] })).toEqual({
      kind: "workflow",
      identifier: "apollia/p/w",
      fields: ["x"],
    });
  });
});

describe("what the examples read of the server", () => {
  test("the address is the one the runtime reports", () => {
    // GIVEN a server on the local network that requires a token and allows request MCP servers
    const read = exampleServer(
      view({ serveOnLan: true, address: "http://192.168.1.20:7773/", port: 7773 }),
      settings({
        [SERVER_SETTING_KEYS.requireAuth]: true,
        [SERVER_SETTING_KEYS.allowRequestMcp]: true,
      }),
    );
    // WHEN the examples read it
    // THEN they get that address without its trailing slash, and the three states
    expect(read).toEqual({
      address: "http://192.168.1.20:7773",
      requireAuth: true,
      serveOnLan: true,
      allowRequestMcp: true,
    });
  });

  test("until the runtime answers, the address is the loopback on the configured port", () => {
    // GIVEN no state read yet, and a port set to 9000
    const read = exampleServer(null, settings({ [SERVER_SETTING_KEYS.port]: 9000 }));
    // WHEN the examples read it
    // THEN they are written for this machine, with the defaults of the settings
    expect(read).toEqual({
      address: "http://127.0.0.1:9000",
      requireAuth: false,
      serveOnLan: false,
      allowRequestMcp: false,
    });
  });
});

describe("the texts and the cards", () => {
  const fr = JSON.parse(source("../../lib/i18n/fr.json")) as Record<string, unknown>;
  const en = JSON.parse(source("../../lib/i18n/en.json")) as Record<string, unknown>;

  function lookup(catalogue: Record<string, unknown>, key: string): unknown {
    return key.split(".").reduce<unknown>((node, part) => {
      if (typeof node !== "object" || node === null) return undefined;
      return (node as Record<string, unknown>)[part];
    }, catalogue);
  }

  test("every label, reason and SDK name exists in both catalogues", () => {
    // GIVEN the keys the menu draws its texts from
    const keys = [
      ...Object.values(EXAMPLE_LABELS),
      ...Object.values(REFUSAL_REASONS),
      ...Object.values(SDK_LABELS),
    ];
    // WHEN each is looked up in French and in English
    // THEN both catalogues hold a text
    for (const key of keys) {
      expect(typeof lookup(fr, key), `${key} (fr)`).toBe("string");
      expect(typeof lookup(en, key), `${key} (en)`).toBe("string");
    }
  });

  test("both kinds of card carry the menu", () => {
    // GIVEN the two sections of cards of the local server
    // WHEN their sources are read
    // THEN each renders the examples menu
    for (const file of ["./ServerModelsSection.svelte", "./ExposedItemsSection.svelte"]) {
      expect(source(file)).toContain("<ExamplesMenu");
    }
  });

  test("no SDK enters the dependencies of the application", () => {
    // GIVEN the manifest of the interface
    const manifest = JSON.parse(source("../../../package.json")) as {
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
    };
    const names = Object.keys({ ...manifest.dependencies, ...manifest.devDependencies });
    // WHEN the SDKs the examples name are looked for
    // THEN none is a dependency: they appear in the text of the examples only
    expect(names).not.toContain("openai");
    expect(names).not.toContain("@anthropic-ai/sdk");
  });
});
