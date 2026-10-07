import { describe, test, expect, vi, beforeAll } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { render as renderToString } from "svelte/server";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

// The tooltip of the application needs the provider mounted at its root and
// shows nothing until hovered. Here it is replaced by a plain wrapper that
// writes what it says next to what it explains, so a test reads both.
type Renderer = { push: (html: string) => void };
vi.mock("$lib/components/ui/tooltip", () => ({
  Tooltip: (
    renderer: Renderer,
    props: {
      content: string;
      triggerTestid?: string;
      triggerLabel?: string;
      children?: (renderer: Renderer) => void;
    },
  ) => {
    const button = props.triggerTestid !== undefined;
    if (button) {
      renderer.push(
        `<button data-testid="${props.triggerTestid}" aria-label="${props.triggerLabel ?? ""}">`,
      );
    }
    props.children?.(renderer);
    renderer.push(`${button ? "</button>" : ""}<div data-tooltip="">${props.content}</div>`);
  },
}));

import HttpReferencePage from "./HttpReferencePage.svelte";
import { breadcrumbOf, neighbours, resolveLink } from "./documentation";
import { createDocumentationReader, type DocumentationReaderDeps } from "./documentationReader";
import {
  HTTP_METHODS,
  buildHttpReference,
  groupTitle,
  operationCount,
  referencePageAt,
  referenceSlug,
  schemaType,
  withHttpReference,
  type HttpReference,
  type ReferencePage,
} from "./httpReference";
import { developerMode } from "$lib/stores/developerMode";
import type { DocumentationManifest } from "$lib/ipc/documentation";

type Json = Record<string, unknown>;

/** The OpenAPI document the clients are generated from, as the archive holds it. */
const CONTRACT_FILE = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../../../../../clients/openapi.json",
);
const CONTRACT = JSON.parse(readFileSync(CONTRACT_FILE, "utf8")) as Json & {
  paths: Record<string, Json>;
};

/** An extract of the document: the two paths of the tasks, with every schema. */
const EXTRACT: Json = {
  ...CONTRACT,
  paths: {
    "/api/v1/tasks": CONTRACT.paths["/api/v1/tasks"],
    "/api/v1/tasks/{id}": CONTRACT.paths["/api/v1/tasks/{id}"],
  },
};

const PUBLIC_SITE = "https://docs.example";

function manifest(language: "fr" | "en" = "fr"): DocumentationManifest {
  return {
    language,
    languages: ["en", "fr"],
    openapi: "openapi.json",
    public_url: language === "fr" ? `${PUBLIC_SITE}/fr` : PUBLIC_SITE,
    pages: {
      "/": { source: "intro.mdx", languages: ["en", "fr"], generated: false },
      "/reference/events": { source: "reference/events.md", languages: ["en"], generated: false },
    },
    tree: [
      { kind: "doc", route: "/", label: "Introduction" },
      {
        kind: "category",
        label: "Reference",
        route: null,
        items: [{ kind: "doc", route: "/reference/events", label: "Events" }],
      },
    ],
    navigation: {
      "/": { kind: "page", title: "Introduction", previous: null, next: null },
      "/reference/events": { kind: "page", title: "Events", previous: null, next: null },
    },
  };
}

const LABELS: Record<string, string> = {
  "dev.developer.docs.reference.title": "HTTP reference",
  "dev.developer.docs.reference.overview": "Overview",
};

function harness(document: unknown = EXTRACT) {
  const calls = { opened: [] as string[], copied: [] as string[], page: [] as string[] };
  const deps: DocumentationReaderDeps = {
    manifest: async (language) => manifest(language.startsWith("fr") ? "fr" : "en"),
    page: async (route, language) => {
      calls.page.push(route);
      return {
        route,
        requested_language: language,
        language,
        fallback: false,
        generated: false,
        markdown: "# Introduction\n\nBody.\n",
        public_url: `${PUBLIC_SITE}/fr${route}`,
      };
    },
    image: async () => new ArrayBuffer(0),
    openapi: async () => {
      if (document instanceof Error) throw document;
      return document;
    },
    label: (key) => LABELS[key] ?? key,
    openUrl: (url) => void calls.opened.push(url),
    writeClipboard: async (text) => void calls.copied.push(text),
    objectUrl: () => "blob:test",
  };
  return { deps, calls };
}

/**
 * Vitest runs in its node environment, where `svelte` resolves to the server
 * build: the markup is what these tests assert on, and the server render walks
 * the same template branches as the browser.
 */
function markupOf(page: ReferencePage): string {
  return renderToString(HttpReferencePage, { props: { page, onopen: () => {} } }).body;
}

function count(markup: string, needle: string): number {
  return markup.split(needle).length - 1;
}

function reference(document: unknown = EXTRACT): HttpReference {
  const built = buildHttpReference(document);
  if (!built) throw new Error("the document declares no operation");
  return built;
}

function groupPage(built: HttpReference, tag: string): ReferencePage {
  const group = built.groups.find((entry) => entry.tag === tag);
  if (!group) throw new Error(`no group ${tag}`);
  return { kind: "group", reference: built, group };
}

beforeAll(async () => {
  developerMode.set(true);
  locale.set("en");
  await waitLocale();
});

describe("the reading of the OpenAPI document", () => {
  test("an extract becomes one group, with the method, the path and the fields of each operation", () => {
    // GIVEN the two paths of the tasks, taken from the published document
    // WHEN the reference is built from them
    const built = reference();

    // THEN one group holds their operations, in the order of the document
    expect(built.groups.map((group) => group.tag)).toEqual(["tasks"]);
    const tasks = built.groups[0];
    expect(tasks.route).toBe("/reference/api/tasks");
    expect(tasks.title).toBe("Tasks");
    expect(tasks.description).toBe("Submit and drive autonomous agent tasks");
    expect(tasks.operations.map((op) => `${op.method} ${op.path}`).slice(0, 3)).toEqual([
      "get /api/v1/tasks",
      "post /api/v1/tasks",
      "get /api/v1/tasks/{id}",
    ]);

    // AND a query parameter is read with its place, its type and its need
    const listing = tasks.operations[0];
    expect(listing.id).toBe("list_tasks");
    expect(listing.summary).toBe("Handler for GET /api/v1/tasks.");
    expect(listing.parameters).toEqual([
      {
        name: "status",
        where: "query",
        type: "string",
        required: false,
        description: "Filter by task status",
      },
    ]);
    expect(listing.body).toBeNull();

    // AND a JSON body is named by its schema, its fields read as parameters
    const submit = tasks.operations[1];
    expect(submit.body).toEqual({
      mediaType: "application/json",
      type: "SubmitTaskRequest",
      required: true,
    });
    const agent = submit.parameters.find((parameter) => parameter.name === "agent_id");
    expect(agent).toEqual({
      name: "agent_id",
      where: "body",
      type: "string",
      required: true,
      description: "Identifier of the target agent.",
    });
    const skill = submit.parameters.find((parameter) => parameter.name === "skill_id");
    expect(skill?.required).toBe(false);
    expect(skill?.type).toBe("string");

    // AND each answer keeps its code, its description and its type
    expect(submit.responses).toEqual([
      { status: "202", description: "Task accepted", type: "TaskResponse" },
      { status: "404", description: "Agent not found", type: "ApiErrorBody" },
      { status: "503", description: "Agent unavailable", type: "ApiErrorBody" },
    ]);

    // AND a path parameter is read as one
    const one = tasks.operations[2];
    expect(one.parameters[0]).toMatchObject({ name: "id", where: "path", required: true });
  });

  test("a type is written as the contract writes it", () => {
    // GIVEN the shapes a schema takes in the document
    // WHEN each is named
    // THEN a reference gives its name, an array its items, a choice its parts
    expect(schemaType({ $ref: "#/components/schemas/TaskResponse" })).toBe("TaskResponse");
    expect(schemaType({ type: "array", items: { type: "integer" } })).toBe("integer[]");
    expect(schemaType({ type: ["string", "null"] })).toBe("string");
    expect(
      schemaType({ oneOf: [{ type: "null" }, { $ref: "#/components/schemas/Budget" }] }),
    ).toBe("Budget");
    expect(schemaType({ anyOf: [{ type: "string" }, { type: "integer" }] })).toBe(
      "string | integer",
    );
    expect(schemaType({ properties: {} })).toBe("object");
    expect(schemaType(undefined)).toBe("any");
  });

  test("a group takes the route the public site serves it at", () => {
    // GIVEN the tags the public site turns into routes
    // WHEN each is given its slug and its title
    // THEN a run of digits is a word, and initials are written in capitals
    expect(referenceSlug("a2a")).toBe("a-2-a");
    expect(referenceSlug("model_hub")).toBe("model-hub");
    expect(referenceSlug("Apollia OS Runtime API")).toBe("apollia-os-runtime-api");
    expect(groupTitle("a2a")).toBe("A2A");
    expect(groupTitle("model_hub")).toBe("Model hub");
    expect(groupTitle("local_ai")).toBe("Local AI");
    expect(reference(CONTRACT).overviewRoute).toBe("/reference/api/apollia-os-runtime-api");
  });

  test("a document without an operation gives no reference", () => {
    // GIVEN what is not an OpenAPI document, or one with no path
    // WHEN the reference is built
    // THEN there is none
    expect(buildHttpReference(null)).toBeNull();
    expect(buildHttpReference({ paths: {} })).toBeNull();
    expect(buildHttpReference({ paths: { "/x": { parameters: [] } } })).toBeNull();
  });

  test("a cycle of references stops the reading of the fields, not the page", () => {
    // GIVEN a body whose schema refers to itself
    const looped = {
      paths: {
        "/loop": {
          post: {
            requestBody: {
              content: { "application/json": { schema: { $ref: "#/components/schemas/A" } } },
            },
            responses: {},
          },
        },
      },
      components: { schemas: { A: { $ref: "#/components/schemas/A" } } },
    };

    // WHEN the reference is built
    const built = reference(looped);

    // THEN the operation is there, in the group of the untagged, with no field
    expect(built.groups[0].tag).toBe("default");
    expect(built.groups[0].operations[0].parameters).toEqual([]);
    expect(built.groups[0].operations[0].body?.type).toBe("A");
  });
});

describe("the page of a group", () => {
  test("shows each operation with its method in a badge, its path, its parameters, its body and its answers", () => {
    // GIVEN the group of the tasks, from the extract
    const built = reference();

    // WHEN its page is rendered
    const markup = markupOf(groupPage(built, "tasks"));

    // THEN one card per operation
    const operations = built.groups[0].operations.length;
    expect(count(markup, 'data-testid="developer-reference-operation"')).toBe(operations);
    expect(count(markup, 'data-testid="developer-reference-method"')).toBe(operations);

    // AND the method is a badge, the path is written whole
    expect(markup).toContain('data-testid="developer-reference-method" aria-label="GET. Reads, and changes nothing."');
    expect(markup).toContain(
      'data-testid="developer-reference-method" aria-label="POST. Creates a resource or starts an action."',
    );
    expect(markup).toContain('<div data-tooltip="">Deletes.</div>');
    expect(markup).toContain("/api/v1/tasks/{id}");

    // AND the parameters sit in a table, each with its place in the request
    expect(markup).toContain("Parameters");
    for (const column of ["Name", "In", "Type", "Required", "Description"]) {
      expect(markup).toContain(`>${column}</th>`);
    }
    expect(markup).toContain(">status</td>");
    expect(markup).toContain(">query</td>");
    expect(markup).toContain(">agent_id</td>");
    expect(markup).toContain(">body</td>");
    expect(markup).toContain(">path</td>");
    expect(markup).toContain("Filter by task status");

    // AND the body is named with its media type, the answers by their code
    expect(markup).toContain("Body, required:");
    expect(markup).toContain("SubmitTaskRequest");
    expect(markup).toContain("(application/json)");
    expect(markup).toContain("Responses");
    expect(markup).toContain(">202</span>");
    expect(markup).toContain("Task accepted");
    expect(markup).toContain("ApiErrorBody");
  });

  test("the overview lists every group with a way to its page", () => {
    // GIVEN the reference of the whole document
    const built = reference(CONTRACT);

    // WHEN the overview is rendered
    const markup = markupOf({ kind: "overview", reference: built });

    // THEN it names the contract and holds one button per group
    expect(markup).toContain("Apollia OS Runtime API");
    expect(count(markup, 'data-testid="developer-reference-group-')).toBe(built.groups.length);
    expect(markup).toContain('data-testid="developer-reference-group-a-2-a"');
    expect(markup).toContain(`${operationCount(built)} operations in ${built.groups.length} groups`);
  });
});

describe("the page outside the developer mode", () => {
  test("renders nothing", () => {
    // GIVEN the developer mode off
    developerMode.set(false);

    // WHEN the page of a group is rendered
    const markup = markupOf(groupPage(reference(), "tasks"));
    developerMode.set(true);

    // THEN no operation is shown
    expect(markup).not.toContain("developer-reference-page");
  });
});

describe("the operations rendered", () => {
  test("are as many as the published document declares", () => {
    // GIVEN the published document, its operations counted from its paths
    let declared = 0;
    for (const item of Object.values(CONTRACT.paths)) {
      for (const method of HTTP_METHODS) if (item[method] !== undefined) declared += 1;
    }
    expect(declared).toBeGreaterThanOrEqual(117);

    // WHEN the page of every group is rendered
    const built = reference(CONTRACT);
    let rendered = 0;
    const seen = new Set<string>();
    for (const group of built.groups) {
      const markup = markupOf({ kind: "group", reference: built, group });
      rendered += count(markup, 'data-testid="developer-reference-operation"');
      for (const operation of group.operations) seen.add(`${operation.method} ${operation.path}`);
    }

    // THEN every operation is on one page and one only
    expect(rendered).toBe(declared);
    expect(operationCount(built)).toBe(declared);
    expect(seen.size).toBe(declared);

    // AND every group has its own route
    expect(new Set(built.groups.map((group) => group.route)).size).toBe(built.groups.length);
  });
});

describe("the reference in the tree", () => {
  test("the tree gains the HTTP reference: the overview, then one page per group", () => {
    // GIVEN a manifest and the reference of the whole document
    const built = reference(CONTRACT);

    // WHEN the reference joins the manifest
    const joined = withHttpReference(manifest(), built, {
      category: "HTTP reference",
      overview: "Overview",
    });

    // THEN the tree ends on its category, the pages of the site untouched
    const last = joined.tree.at(-1);
    expect(joined.tree.length).toBe(manifest().tree.length + 1);
    expect(last).toMatchObject({ kind: "category", label: "HTTP reference", route: null });
    const items = last?.kind === "category" ? last.items : [];
    expect(items[0]).toEqual({
      kind: "doc",
      route: "/reference/api/apollia-os-runtime-api",
      label: "Overview",
    });
    expect(items.length).toBe(built.groups.length + 1);

    // AND the breadcrumb and the neighbours of a group read from it
    expect(breadcrumbOf(joined.tree, "/reference/api/tasks").map((step) => step.label)).toEqual([
      "HTTP reference",
      "Tasks",
    ]);
    const first = built.groups[0];
    expect(neighbours(joined, first.route).previous).toEqual({
      route: built.overviewRoute,
      title: "Overview",
    });
    expect(neighbours(joined, built.overviewRoute).previous).toBeNull();
    expect(neighbours(joined, built.groups.at(-1)?.route ?? "").next).toBeNull();

    // AND a link a page writes to the reference now opens in the pane
    expect(resolveLink(joined, "/", "/reference/api/apollia-os-runtime-api")).toEqual({
      kind: "page",
      route: "/reference/api/apollia-os-runtime-api",
      anchor: null,
    });
  });

  test("the reader shows a page of the reference without asking the shell for a source", async () => {
    // GIVEN a reader over an archive that holds the extract
    const { deps, calls } = harness();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");

    // WHEN the page of the tasks is opened
    await reader.open("/reference/api/tasks");

    // THEN the reader shows the group, under its title, with no source read
    const state = get(reader);
    expect(state.reference?.kind).toBe("group");
    expect(state.view?.title).toBe("Tasks");
    expect(state.page).toBeNull();
    expect(state.failed).toBe(false);
    expect(calls.page).toEqual(["/"]);

    // AND a page of the site takes the reference away again
    await reader.open("/");
    expect(get(reader).reference).toBeNull();
    expect(referencePageAt(null, "/reference/api/tasks")).toBeNull();
  });

  test("an archive whose document cannot be read keeps its documentation", async () => {
    // GIVEN a reader whose OpenAPI document cannot be read
    const { deps } = harness(new Error("the embedded documentation cannot be opened"));
    const reader = createDocumentationReader(deps);

    // WHEN the documentation is started
    await reader.start("fr");

    // THEN the tree is the one of the site, and the first page is shown
    const state = get(reader);
    expect(state.manifest?.tree.length).toBe(manifest().tree.length);
    expect(state.failed).toBe(false);
    expect(state.view?.title).toBe("Introduction");
  });
});

describe("the gestures on a generated page", () => {
  test("View on the web opens the public reference, in the language read", async () => {
    // GIVEN a reader on the page of the tasks, in French
    const { deps, calls } = harness();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");
    await reader.open("/reference/api/tasks");

    // WHEN the page is asked for on the web
    await reader.viewOnWeb();

    // THEN the browser opens the page of the public reference
    expect(calls.opened).toEqual([`${PUBLIC_SITE}/fr/reference/api/tasks`]);

    // AND the overview opens the one the public site names after the contract
    await reader.open("/reference/api/apollia-os-runtime-api");
    expect(reader.webUrl()).toBe(`${PUBLIC_SITE}/fr/reference/api/apollia-os-runtime-api`);
  });

  test("a generated page has no source to copy", async () => {
    // GIVEN a reader on the page of the tasks
    const { deps, calls } = harness();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");
    await reader.open("/reference/api/tasks");

    // WHEN the copy is asked for all the same
    const copied = await reader.copyMarkdown();

    // THEN nothing reaches the clipboard
    expect(copied).toBe(false);
    expect(calls.copied).toEqual([]);
  });
});
