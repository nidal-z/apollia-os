import { describe, test, expect, vi, beforeAll } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { render as renderToString } from "svelte/server";
import type { Component } from "svelte";
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

const shell = vi.hoisted(() => ({ answers: new Map<string, unknown>() }));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string) => shell.answers.get(command) ?? null),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

// The node environment has no DOM for the sanitizer; this suite reads the
// markup around a page of the site, not the page.
vi.mock("dompurify", () => ({
  default: { isSupported: false, sanitize: (html: string) => html, addHook: () => {} },
}));

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

import DocumentationPane from "./DocumentationPane.svelte";
import { documentationReader } from "./documentationReader";
import { developerMode } from "$lib/stores/developerMode";
import type { DocumentationManifest } from "$lib/ipc/documentation";

type Json = Record<string, unknown>;

/** The OpenAPI document the clients are generated from, as the archive holds it. */
const CONTRACT = JSON.parse(
  readFileSync(
    path.resolve(
      path.dirname(fileURLToPath(import.meta.url)),
      "../../../../../../clients/openapi.json",
    ),
    "utf8",
  ),
) as Json & { paths: Record<string, Json> };

/** An extract of the document: the path of the tasks, with every schema. */
const EXTRACT: Json = {
  ...CONTRACT,
  paths: { "/api/v1/tasks": CONTRACT.paths["/api/v1/tasks"] },
};

const PUBLIC_SITE = "https://docs.example";

function manifest(): DocumentationManifest {
  return {
    language: "fr",
    languages: ["en", "fr"],
    openapi: "openapi.json",
    public_url: `${PUBLIC_SITE}/fr`,
    pages: { "/": { source: "intro.md", languages: ["en", "fr"], generated: false } },
    tree: [{ kind: "doc", route: "/", label: "Introduction" }],
    navigation: { "/": { kind: "page", title: "Introduction", previous: null, next: null } },
  };
}

/**
 * Vitest runs in its node environment, where `svelte` resolves to the server
 * build: the markup is what this suite asserts on, and the server render walks
 * the same template branches as the browser.
 */
function shown(): string {
  return renderToString(DocumentationPane as Component<Record<string, unknown>>, { props: {} })
    .body;
}

function has(markup: string, testid: string): boolean {
  return markup.includes(`data-testid="${testid}"`);
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

describe("the documentation pane on a generated page", () => {
  test("the pane shows no Copy as Markdown on a generated page, and says why on hover", async () => {
    // GIVEN the pane of the application, developer mode on, over an archive
    // that holds the extract
    shell.answers.set("get_documentation_manifest", manifest());
    shell.answers.set("get_documentation_openapi", EXTRACT);
    shell.answers.set("get_documentation_page", {
      route: "/",
      requested_language: "fr",
      language: "fr",
      fallback: false,
      generated: false,
      markdown: "# Introduction\n\nBody.\n",
      public_url: `${PUBLIC_SITE}/fr/`,
    });
    developerMode.set(true);
    await documentationReader.start("fr");

    // WHEN a page of the site is shown
    const written = shown();

    // THEN the copy button is there
    expect(has(written, "developer-docs-copy")).toBe(true);
    expect(has(written, "developer-docs-copy-absent")).toBe(false);
    expect(has(written, "developer-reference-page")).toBe(false);

    // WHEN a page of the reference is shown
    await documentationReader.open("/reference/api/tasks");
    const generated = shown();

    // THEN the copy button is gone, and what stands in its place is a button
    // that carries the reason, which is what its hover shows
    const reason =
      "Page generated from the API reference: there is no Markdown source to copy.";
    expect(has(generated, "developer-docs-copy")).toBe(false);
    const mark = /<button[^>]*data-testid="developer-docs-copy-absent"[^>]*>/.exec(generated)?.[0];
    expect(mark).toContain(`aria-label="${reason}"`);
    expect(generated).toContain(`<div data-tooltip="">${reason}</div>`);

    // AND the page is the reference, under its breadcrumb and its title
    expect(has(generated, "developer-reference-page")).toBe(true);
    expect(generated.split('data-testid="developer-reference-operation"').length - 1).toBe(2);
    expect(generated).toContain("HTTP reference");
    expect(generated).toMatch(/data-testid="developer-docs-title"[^>]*>\s*Tasks\s*</);

    // AND View on the web is still offered, on the public reference
    expect(has(generated, "developer-docs-web")).toBe(true);
    expect(documentationReader.webUrl()).toBe(`${PUBLIC_SITE}/fr/reference/api/tasks`);
  });
});
