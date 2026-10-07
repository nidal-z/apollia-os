// The call examples of a card of the local server: what the menu lists for a
// model, an agent or a workflow, which entries it greys out and why, and the
// text of each example.
//
// An example is a pure function of what is called, of the address of the
// server and of whether the call must present a token. A token is always
// written as the variable `APOLLIA_TOKEN`, never as a value. The SDKs named
// in the Python and JavaScript examples are those of the caller: they appear
// in the text only.

import type { LocalServerView } from "$lib/ipc/localServer";
import {
  SERVER_SETTING_KEYS,
  type ServerSetting,
  type ServerSettingKey,
} from "$lib/stores/serverSettings";
import type { ServedInputs, ServedModelKind } from "./servedItems";

/** The entries of the menu, in reading order. */
export const EXAMPLE_IDS = ["curl_simple", "curl_mcp", "curl_tool", "python", "javascript"] as const;

/** One entry of the menu. */
export type ExampleId = (typeof EXAMPLE_IDS)[number];

/** The SDK a Python or JavaScript example is written with. */
export type ExampleSdk = "openai" | "anthropic";

/** The i18n key of the name of each SDK, in the choice of the preview. */
export const SDK_LABELS: Record<ExampleSdk, string> = {
  openai: "dev.developer.examples.sdk_openai",
  anthropic: "dev.developer.examples.sdk_anthropic",
};

/** The environment variable every example reads the token from. */
export const TOKEN_VARIABLE = "APOLLIA_TOKEN";

/** What the OpenAI and Anthropic SDKs are given as a key when the server asks for none. */
export const NO_TOKEN = "not-needed";

/** The address of the MCP server the example brings, to be replaced by the reader's. */
export const EXAMPLE_MCP_URL = "http://localhost:3000/mcp";

/** What an example calls. */
export type ExampleTarget =
  | { kind: ServedModelKind; identifier: string }
  | { kind: "agent"; identifier: string }
  /** A workflow takes a JSON object with these fields. */
  | { kind: "workflow"; identifier: string; fields: string[] };

/** The target of the examples of an exposed agent or workflow. */
export function exposedTarget(
  kind: "agent" | "workflow",
  identifier: string,
  inputs: ServedInputs,
): ExampleTarget {
  if (kind === "agent") return { kind, identifier };
  return { kind, identifier, fields: inputs.kind === "fields" ? inputs.names : [] };
}

/** What the examples read of the server. */
export interface ExampleServer {
  /** The address a client reaches the server at, without a trailing slash. */
  address: string;
  /** Whether every request must present a token. */
  requireAuth: boolean;
  /** Whether the address is one of the local network. */
  serveOnLan: boolean;
  /** Whether a request may bring an MCP server. */
  allowRequestMcp: boolean;
}

/**
 * The server as the examples read it: the address the runtime reports, or,
 * until it answers, the loopback address on the configured port.
 */
export function exampleServer(
  view: LocalServerView | null,
  settings: Record<ServerSettingKey, ServerSetting>,
): ExampleServer {
  const port = view?.port ?? settings[SERVER_SETTING_KEYS.port].value;
  const address = view?.address ?? `http://127.0.0.1:${port}`;
  return {
    address: address.replace(/\/+$/, ""),
    requireAuth: settings[SERVER_SETTING_KEYS.requireAuth].value === true,
    serveOnLan: view?.serveOnLan ?? settings[SERVER_SETTING_KEYS.serveOnLan].value === true,
    allowRequestMcp: settings[SERVER_SETTING_KEYS.allowRequestMcp].value === true,
  };
}

/** Why an entry of the menu is greyed out. */
export type ExampleRefusal =
  /** Only a model takes tools: an agent or a workflow has its own accesses. */
  | "models_only"
  /** The model answers embeddings, not a conversation. */
  | "chat_only"
  /** The setting that lets a request bring an MCP server is off. */
  | "mcp_off";

/** The i18n key of the reason shown on hover of a greyed entry. */
export const REFUSAL_REASONS: Record<ExampleRefusal, string> = {
  models_only: "dev.developer.examples.reason_models_only",
  chat_only: "dev.developer.examples.reason_chat_only",
  mcp_off: "dev.developer.examples.reason_mcp_off",
};

/** The i18n key of the name of each entry. */
export const EXAMPLE_LABELS: Record<ExampleId, string> = {
  curl_simple: "dev.developer.examples.curl_simple",
  curl_mcp: "dev.developer.examples.curl_mcp",
  curl_tool: "dev.developer.examples.curl_tool",
  python: "dev.developer.examples.python",
  javascript: "dev.developer.examples.javascript",
};

/**
 * Why `id` cannot be used on `target`, `null` when it can. The server
 * refuses tools, and an MCP server brought by the request, on an agent or a
 * workflow; an embeddings model answers neither.
 */
export function exampleRefusal(
  id: ExampleId,
  target: ExampleTarget,
  server: ExampleServer,
): ExampleRefusal | null {
  if (id !== "curl_mcp" && id !== "curl_tool") return null;
  if (target.kind === "agent" || target.kind === "workflow") return "models_only";
  if (target.kind === "embeddings") return "chat_only";
  if (id === "curl_mcp" && !server.allowRequestMcp) return "mcp_off";
  return null;
}

/** One entry of the menu as it is drawn. */
export interface ExampleEntry {
  id: ExampleId;
  /** The i18n key of its name. */
  label: string;
  /** The i18n key of why it is greyed out, `null` when it can be copied. */
  reason: string | null;
}

/** The five entries of the menu of `target`; one that cannot be used stays, greyed. */
export function exampleEntries(target: ExampleTarget, server: ExampleServer): ExampleEntry[] {
  return EXAMPLE_IDS.map((id) => {
    const refusal = exampleRefusal(id, target, server);
    return { id, label: EXAMPLE_LABELS[id], reason: refusal ? REFUSAL_REASONS[refusal] : null };
  });
}

/**
 * The SDKs `id` is offered with: both for a Python or JavaScript example,
 * the OpenAI one alone for an embeddings model (the Messages API has no
 * embeddings), none for a cURL example.
 */
export function exampleSdks(id: ExampleId, target: ExampleTarget): ExampleSdk[] {
  if (id !== "python" && id !== "javascript") return [];
  return target.kind === "embeddings" ? ["openai"] : ["openai", "anthropic"];
}

/** The SDK an example is shown with: the one chosen when `id` offers it. */
export function chosenSdk(id: ExampleId, target: ExampleTarget, chosen: ExampleSdk): ExampleSdk {
  return exampleSdks(id, target).includes(chosen) ? chosen : "openai";
}

/**
 * Whether the example presents a token: when the server asks every request
 * for one, and, on the local network, for an agent, a workflow or an MCP
 * server brought by the request, which never answer the network without one.
 */
export function presentsToken(id: ExampleId, target: ExampleTarget, server: ExampleServer): boolean {
  if (server.requireAuth) return true;
  if (!server.serveOnLan) return false;
  return target.kind === "agent" || target.kind === "workflow" || id === "curl_mcp";
}

/** The words the examples send, in the language of the reader. */
export interface ExampleTexts {
  /** What the simple examples ask. */
  prompt: string;
  /** A question the tool of the tool example answers. */
  toolPrompt: string;
  /** What the MCP example asks. */
  mcpPrompt: string;
}

/** Everything an example is a function of. */
export interface ExampleAsk {
  id: ExampleId;
  target: ExampleTarget;
  /** The address of the server, without a trailing slash. */
  address: string;
  /** Whether the call presents a token. */
  token: boolean;
  /** The SDK of a Python or JavaScript example; left aside by cURL. */
  sdk: ExampleSdk;
  texts: ExampleTexts;
}

type Json = string | number | Json[] | { [key: string]: Json };

/** What the user message holds: the words, or for a workflow its inputs as JSON. */
function userContent(target: ExampleTarget, words: string): string {
  if (target.kind !== "workflow") return words;
  const inputs: Record<string, string> = {};
  for (const field of target.fields) inputs[field] = "...";
  return JSON.stringify(inputs);
}

function userMessages(target: ExampleTarget, words: string): Json {
  return [{ role: "user", content: userContent(target, words) }];
}

/** The tool of the tool example. */
const WEATHER_TOOL: Json = {
  type: "function",
  function: {
    name: "get_weather",
    parameters: {
      type: "object",
      properties: { city: { type: "string" } },
      required: ["city"],
    },
  },
};

interface CurlCall {
  path: string;
  body: Json;
}

function curlCall(ask: ExampleAsk): CurlCall {
  const model = ask.target.identifier;
  if (ask.id === "curl_mcp") {
    return {
      path: "/v1/responses",
      body: {
        model,
        input: ask.texts.mcpPrompt,
        tools: [{ type: "mcp", server_label: "example", server_url: EXAMPLE_MCP_URL }],
      },
    };
  }
  if (ask.id === "curl_tool") {
    return {
      path: "/v1/chat/completions",
      body: {
        model,
        messages: userMessages(ask.target, ask.texts.toolPrompt),
        tools: [WEATHER_TOOL],
      },
    };
  }
  if (ask.target.kind === "embeddings") {
    return { path: "/v1/embeddings", body: { model, input: ask.texts.prompt } };
  }
  return {
    path: "/v1/chat/completions",
    body: { model, messages: userMessages(ask.target, ask.texts.prompt) },
  };
}

/** `text` inside single quotes of a POSIX shell. */
function shellQuoted(text: string): string {
  return `'${text.replace(/'/g, "'\\''")}'`;
}

/** Every line after the first pushed right by `indent`. */
function hanging(text: string, indent: string): string {
  return text.split("\n").join(`\n${indent}`);
}

function curlExample(ask: ExampleAsk): string {
  const call = curlCall(ask);
  const lines = [`curl ${ask.address}${call.path}`, '-H "Content-Type: application/json"'];
  if (ask.token) lines.push(`-H "Authorization: Bearer $${TOKEN_VARIABLE}"`);
  lines.push(`-d ${shellQuoted(hanging(JSON.stringify(call.body, null, 2), "  "))}`);
  return lines.join(" \\\n  ");
}

/** A JSON value as a literal Python and JavaScript both read, one line. */
function literal(value: Json): string {
  if (Array.isArray(value)) return `[${value.map(literal).join(", ")}]`;
  if (typeof value === "object") {
    const fields = Object.entries(value).map(([key, item]) => `${JSON.stringify(key)}: ${literal(item)}`);
    return `{${fields.join(", ")}}`;
  }
  return JSON.stringify(value);
}

function pythonExample(ask: ExampleAsk): string {
  const model = literal(ask.target.identifier);
  const key = ask.token ? `os.environ["${TOKEN_VARIABLE}"]` : `"${NO_TOKEN}"`;
  const head = ask.token ? ["import os", ""] : [];
  if (ask.sdk === "anthropic") {
    return [
      ...head,
      "from anthropic import Anthropic",
      "",
      "client = Anthropic(",
      `    base_url="${ask.address}",`,
      `    api_key=${key},`,
      ")",
      "message = client.messages.create(",
      `    model=${model},`,
      "    max_tokens=1024,",
      `    messages=${literal(userMessages(ask.target, ask.texts.prompt))},`,
      ")",
      "print(message.content[-1].text)",
    ].join("\n");
  }
  const client = [
    ...head,
    "from openai import OpenAI",
    "",
    "client = OpenAI(",
    `    base_url="${ask.address}/v1",`,
    `    api_key=${key},`,
    ")",
  ];
  if (ask.target.kind === "embeddings") {
    return [
      ...client,
      "response = client.embeddings.create(",
      `    model=${model},`,
      `    input=${literal(ask.texts.prompt)},`,
      ")",
      "print(len(response.data[0].embedding))",
    ].join("\n");
  }
  return [
    ...client,
    "response = client.chat.completions.create(",
    `    model=${model},`,
    `    messages=${literal(userMessages(ask.target, ask.texts.prompt))},`,
    ")",
    "print(response.choices[0].message.content)",
  ].join("\n");
}

function javascriptExample(ask: ExampleAsk): string {
  const model = literal(ask.target.identifier);
  const key = ask.token ? `process.env.${TOKEN_VARIABLE}` : `"${NO_TOKEN}"`;
  if (ask.sdk === "anthropic") {
    return [
      'import Anthropic from "@anthropic-ai/sdk";',
      "",
      "const client = new Anthropic({",
      `  baseURL: "${ask.address}",`,
      `  apiKey: ${key},`,
      "});",
      "const message = await client.messages.create({",
      `  model: ${model},`,
      "  max_tokens: 1024,",
      `  messages: ${literal(userMessages(ask.target, ask.texts.prompt))},`,
      "});",
      "console.log(message.content.at(-1).text);",
    ].join("\n");
  }
  const client = [
    'import OpenAI from "openai";',
    "",
    "const client = new OpenAI({",
    `  baseURL: "${ask.address}/v1",`,
    `  apiKey: ${key},`,
    "});",
  ];
  if (ask.target.kind === "embeddings") {
    return [
      ...client,
      "const response = await client.embeddings.create({",
      `  model: ${model},`,
      `  input: ${literal(ask.texts.prompt)},`,
      "});",
      "console.log(response.data[0].embedding.length);",
    ].join("\n");
  }
  return [
    ...client,
    "const response = await client.chat.completions.create({",
    `  model: ${model},`,
    `  messages: ${literal(userMessages(ask.target, ask.texts.prompt))},`,
    "});",
    "console.log(response.choices[0].message.content);",
  ].join("\n");
}

/** The text of an example, ready to paste. */
export function exampleText(ask: ExampleAsk): string {
  if (ask.id === "python") return pythonExample(ask);
  if (ask.id === "javascript") return javascriptExample(ask);
  return curlExample(ask);
}

/**
 * The example `id` of `target` against `server`, with the SDK chosen when
 * the example offers it; `null` for an entry the menu greys out.
 */
export function exampleFor(
  id: ExampleId,
  target: ExampleTarget,
  server: ExampleServer,
  sdk: ExampleSdk,
  texts: ExampleTexts,
): string | null {
  if (exampleRefusal(id, target, server)) return null;
  return exampleText({
    id,
    target,
    address: server.address,
    token: presentsToken(id, target, server),
    sdk: chosenSdk(id, target, sdk),
    texts,
  });
}

/** The entry the preview opens on: the first, which every target can use. */
export const FIRST_EXAMPLE: ExampleId = "curl_simple";

/** What the test ids of an entry and of its copy button end with. */
export function exampleSlug(id: ExampleId): string {
  return id.replace(/_/g, "-");
}
