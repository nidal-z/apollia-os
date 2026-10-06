import { describe, test, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

import { get } from "svelte/store";
import {
  admonitionsToQuotes,
  breadcrumbOf,
  categoriesHolding,
  categoryIndex,
  categoryOpen,
  categorySlug,
  entrySlug,
  firstRoute,
  headingSlug,
  imageType,
  mdxToMarkdown,
  neighbours,
  preparePage,
  resolveLink,
  webAddress,
} from "./documentation";
import { createDocumentationReader, type DocumentationReaderDeps } from "./documentationReader";
import type { DocumentationManifest, DocumentationPage } from "$lib/ipc/documentation";

// ── A small documentation, shaped like the manifest the shell serves ──────

/** The site the fixture publishes on, unless a test names another. */
const FIXTURE_SITE = "https://docs.example";

/** The manifest of the fixture as `language` shows it. */
function fixtureManifest(
  language: "fr" | "en" = "fr",
  site: string = FIXTURE_SITE,
): DocumentationManifest {
  const fr = language === "fr";
  const title = (french: string, english: string) => (fr ? french : english);
  const install = title("Installer et lancer", "Install and run");
  const audit = title("Auditer un lancement", "Audit and verify");
  const sdk = "LLM";
  return {
    language,
    languages: ["en", "fr"],
    openapi: "openapi.json",
    public_url: fr ? `${site}/fr` : site,
    pages: {
      "/": { source: "intro.mdx", languages: ["en", "fr"], generated: false },
      "/how-to/": { source: "how-to/index.md", languages: ["en", "fr"], generated: false },
      "/how-to/install-and-run": {
        source: "how-to/install-and-run.md",
        languages: ["en", "fr"],
        generated: false,
      },
      "/how-to/audit-and-verify": {
        source: "how-to/audit-and-verify.md",
        languages: ["en", "fr"],
        generated: false,
      },
      "/reference/sdk/llm": { source: "reference/sdk/llm.md", languages: ["en"], generated: true },
      "/explanation/plugins": { source: "explanation/plugins.md", languages: ["en"], generated: false },
    },
    tree: [
      { kind: "doc", route: "/", label: "Documentation" },
      {
        kind: "category",
        label: title("Guides pratiques", "How-to guides"),
        route: "/how-to/",
        items: [
          { kind: "doc", route: "/how-to/install-and-run", label: install },
          { kind: "doc", route: "/how-to/audit-and-verify", label: audit },
        ],
      },
      {
        kind: "category",
        label: title("Manuel", "Reference"),
        route: null,
        items: [
          {
            kind: "category",
            label: "SDK",
            route: "/reference/sdk",
            items: [{ kind: "doc", route: "/reference/sdk/llm", label: sdk }],
          },
        ],
      },
      { kind: "doc", route: "/explanation/plugins", label: "Plugins" },
    ],
    navigation: {
      "/": {
        kind: "page",
        title: "Documentation",
        previous: null,
        next: { route: "/how-to/", title: title("Guides pratiques", "How-to guides") },
      },
      "/how-to/": {
        kind: "page",
        title: title("Guides pratiques", "How-to guides"),
        previous: { route: "/", title: "Documentation" },
        next: { route: "/how-to/install-and-run", title: install },
      },
      "/how-to/install-and-run": {
        kind: "page",
        title: install,
        previous: { route: "/how-to/", title: title("Guides pratiques", "How-to guides") },
        next: { route: "/how-to/audit-and-verify", title: audit },
      },
      "/how-to/audit-and-verify": {
        kind: "page",
        title: audit,
        previous: { route: "/how-to/install-and-run", title: install },
        next: { route: "/reference/sdk", title: "SDK" },
      },
      "/reference/sdk": {
        kind: "generated-index",
        title: "SDK",
        previous: { route: "/how-to/audit-and-verify", title: audit },
        next: { route: "/reference/sdk/llm", title: sdk },
      },
      "/reference/sdk/llm": {
        kind: "page",
        title: sdk,
        previous: { route: "/reference/sdk", title: "SDK" },
        next: { route: "/explanation/plugins", title: "Plugins" },
      },
      "/explanation/plugins": {
        kind: "page",
        title: "Plugins",
        previous: { route: "/reference/sdk/llm", title: sdk },
        next: null,
      },
    },
  };
}

/** The page the shell serves at `route` for `markdown`, asked in `language`. */
function fixturePage(
  route: string,
  markdown: string,
  language: "fr" | "en" = "fr",
  served: "fr" | "en" = language,
  site: string = FIXTURE_SITE,
): DocumentationPage {
  const manifest = fixtureManifest(language, site);
  const generated = manifest.pages[route]?.generated ?? false;
  return {
    route,
    requested_language: language,
    language: served,
    fallback: served !== language && !generated,
    generated,
    markdown,
    public_url: `${manifest.public_url}${route}`,
  };
}

/** The public site, as the manifest of the application names it. */
const PUBLIC_SITE = "https://docs.apollia.fr";

/** The documentation site of this repository. */
const SITE = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../../../../../docs/site",
);

function siteFile(language: "fr" | "en", source: string): string {
  const root =
    language === "fr" ? "i18n/fr/docusaurus-plugin-content-docs/current" : "docs";
  return readFileSync(path.join(SITE, root, source), "utf8");
}

/**
 * The shell behind the reader, held in memory: every call is recorded, a page
 * without a French mirror is served in English, and nothing else is reachable.
 */
function fakeShell(sources: Record<string, string> = {}) {
  const calls = {
    manifest: [] as string[],
    page: [] as [string, string][],
    image: [] as string[],
    opened: [] as string[],
    copied: [] as string[],
  };
  const deps: DocumentationReaderDeps = {
    manifest: async (language) => {
      calls.manifest.push(language);
      return fixtureManifest(language.startsWith("fr") ? "fr" : "en", PUBLIC_SITE);
    },
    page: async (route, language) => {
      calls.page.push([route, language]);
      const asked = language.startsWith("fr") ? "fr" : "en";
      const info = fixtureManifest(asked, PUBLIC_SITE).pages[route];
      if (!info) throw new Error(`no documentation page at ${route}`);
      const served = info.languages.includes(asked) ? asked : "en";
      return fixturePage(
        route,
        sources[route] ?? `# ${route}\n\nBody.\n`,
        asked,
        served,
        PUBLIC_SITE,
      );
    },
    image: async (file) => {
      calls.image.push(file);
      if (file.includes("missing")) throw new Error("no documentation image");
      return new ArrayBuffer(4);
    },
    openUrl: (url) => {
      calls.opened.push(url);
    },
    writeClipboard: async (text) => {
      calls.copied.push(text);
    },
    objectUrl: (_bytes, type) => `blob:${type}`,
  };
  return { deps, calls };
}

describe("the tree of the documentation", () => {
  test("opens on the first page and lists every entry under its category", async () => {
    // GIVEN a reader over the manifest the shell serves
    const { deps, calls } = fakeShell();
    const reader = createDocumentationReader(deps);

    // WHEN the documentation is read in French
    await reader.start("fr");

    // THEN the tree is the manifest's, and the first page of it is shown
    const state = get(reader);
    expect(calls.manifest).toEqual(["fr"]);
    expect(state.manifest?.tree.map((item) => item.label)).toEqual([
      "Documentation",
      "Guides pratiques",
      "Manuel",
      "Plugins",
    ]);
    expect(firstRoute(state.manifest?.tree ?? [])).toBe("/");
    expect(state.route).toBe("/");
  });

  test("opens by itself the categories that hold the page read, and obeys the user", () => {
    // GIVEN the tree, and a page two categories deep
    const { tree } = fixtureManifest();

    // WHEN the categories that hold it are asked for
    const holding = categoriesHolding(tree, "/reference/sdk/llm");

    // THEN both are open, the others closed, and a gesture overrides either
    expect(holding).toEqual(["2", "2.0"]);
    expect(categoryOpen("2.0", holding, {})).toBe(true);
    expect(categoryOpen("1", holding, {})).toBe(false);
    expect(categoryOpen("1", holding, { "1": true })).toBe(true);
    expect(categoryOpen("2", holding, { "2": false })).toBe(false);
    expect(categoriesHolding(tree, "/how-to/")).toEqual(["1"]);
  });

  test("names each entry for the automation by its route", () => {
    expect(entrySlug("/")).toBe("home");
    expect(entrySlug("/how-to/")).toBe("how-to");
    expect(entrySlug("/how-to/install-and-run")).toBe("how-to-install-and-run");
    expect(categorySlug("2.0")).toBe("2-0");
  });

  test("shows a category without a source as the list of its entries", async () => {
    // GIVEN a category the site generates the page of
    const { deps, calls } = fakeShell();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");
    calls.page.length = 0;

    // WHEN it is opened
    await reader.open("/reference/sdk");

    // THEN its entries are listed without asking the shell for a source,
    // and nothing is offered to copy
    const state = get(reader);
    expect(calls.page).toEqual([]);
    expect(state.view).toEqual({ title: "SDK", body: "- [LLM](/reference/sdk/llm)" });
    expect(state.page).toBeNull();
    expect(await reader.copyMarkdown()).toBe(false);
    expect(categoryIndex(fixtureManifest().tree, "/nowhere")).toBe("");
  });
});

describe("the breadcrumb, previous and next", () => {
  test("the breadcrumb walks the categories down to the page", () => {
    // GIVEN the tree
    const { tree } = fixtureManifest();

    // WHEN the way to a page is asked for
    // THEN it names each category, with its route when it has a page
    expect(breadcrumbOf(tree, "/how-to/install-and-run")).toEqual([
      { label: "Guides pratiques", route: "/how-to/" },
      { label: "Installer et lancer", route: "/how-to/install-and-run" },
    ]);
    expect(breadcrumbOf(tree, "/reference/sdk/llm")).toEqual([
      { label: "Manuel", route: null },
      { label: "SDK", route: "/reference/sdk" },
      { label: "LLM", route: "/reference/sdk/llm" },
    ]);
    expect(breadcrumbOf(tree, "/how-to/")).toEqual([
      { label: "Guides pratiques", route: "/how-to/" },
    ]);
    expect(breadcrumbOf(tree, "/nowhere")).toEqual([]);
  });

  test("previous is the page before in reading order, and the first page has none", () => {
    // GIVEN the manifest
    const manifest = fixtureManifest();

    // WHEN the neighbours of a page and of the first page are read
    // THEN previous is the manifest's, and absent at the start
    expect(neighbours(manifest, "/how-to/audit-and-verify").previous).toEqual({
      route: "/how-to/install-and-run",
      title: "Installer et lancer",
    });
    expect(neighbours(manifest, "/").previous).toBeNull();
  });

  test("next is the page after in reading order, and the last page has none", () => {
    // GIVEN the manifest
    const manifest = fixtureManifest();

    // WHEN the neighbours of a page and of the last page are read
    // THEN next is the manifest's, and absent at the end
    expect(neighbours(manifest, "/how-to/install-and-run").next).toEqual({
      route: "/how-to/audit-and-verify",
      title: "Auditer un lancement",
    });
    expect(neighbours(manifest, "/explanation/plugins").next).toBeNull();
    expect(neighbours(manifest, "/nowhere")).toEqual({ previous: null, next: null });
  });
});

describe("the source of a page, made renderable", () => {
  test("the front matter leaves, the first heading becomes the title", () => {
    // GIVEN a page as the site stores it
    const page = fixturePage(
      "/how-to/install-and-run",
      "---\ntitle: Install\nsidebar_position: 2\n---\n\n# Install and run {#install}\n\nFirst line.\n",
    );

    // WHEN it is prepared
    const view = preparePage(page, { mdx: false, fallbackTitle: "From the tree" });

    // THEN the title is the heading without its identifier, the body what follows
    expect(view).toEqual({ title: "Install and run", body: "First line." });
  });

  test("a page that opens on no heading takes its title from the tree", () => {
    const page = fixturePage("/how-to/", "---\ntitle: x\n---\nJust text.\n");
    expect(preparePage(page, { mdx: false, fallbackTitle: "Guides pratiques" })).toEqual({
      title: "Guides pratiques",
      body: "Just text.",
    });
  });

  test("an admonition becomes a quote that opens on its kind and its title", () => {
    // GIVEN the admonitions of the site, one titled, one not, one in a code block
    const source = [
      ":::warning Reset Apollia entirely",
      "First paragraph.",
      "",
      "- an item",
      ":::",
      "",
      ":::note",
      "Plain.",
      ":::",
      "",
      "```md",
      ":::tip",
      "stays as written",
      ":::",
      "```",
    ].join("\n");

    // WHEN they are rewritten
    const out = admonitionsToQuotes(source);

    // THEN each is a quote marked with its kind, and the code is untouched
    expect(out).toContain("> [!WARNING] Reset Apollia entirely\n>\n> First paragraph.\n>\n> - an item\n");
    expect(out).toContain("> [!NOTE]\n>\n> Plain.\n");
    expect(out).toContain("```md\n:::tip\nstays as written\n:::\n```");
    expect(out).not.toMatch(/^:::warning/m);
  });

  test("the MDX home page of the site reads as plain Markdown", () => {
    // GIVEN the one MDX page of the site, in both languages
    for (const language of ["en", "fr"] as const) {
      const page = fixturePage("/", siteFile(language, "intro.mdx"), language);

      // WHEN it is prepared as an MDX source
      const view = preparePage(page, { mdx: true, fallbackTitle: "Documentation" });

      // THEN no import, no comment and no component is left, the hero heading
      // is the title, and each card is a link the reader can follow
      expect(view.body).not.toMatch(/^import\s/m);
      expect(view.body).not.toContain("{/*");
      expect(view.body).not.toMatch(/<\/?(?:Link|div|img|span|strong|p|h1)\b/);
      expect(view.body).not.toContain("className");
      expect(view.body).not.toContain("useBaseUrl");
      expect(view.title).not.toBe("");
      expect(view.title).not.toBe("Documentation");
      expect(view.body).toMatch(/^- \*\*\[[^\]]+\]\(\/tutorials\)\*\* {2}\n {2}\S/m);
      expect(view.body).toMatch(/^## /m);
      expect(view.body).not.toMatch(/^ {4,}\S/m);
    }
  });

  test("a component written in code is left as written", () => {
    const source = "Use `<Link to=\"/x\">` here.\n\n```jsx\n<Link to=\"/y\">Y</Link>\n```\n";
    expect(mdxToMarkdown(source)).toBe(source);
  });

  test("a heading takes the identifier the site gives it", () => {
    expect(headingSlug("What's new")).toBe("whats-new");
    expect(headingSlug("Routing: local & remote")).toBe("routing-local--remote");
  });

  test("an image of the archive has the type of its extension", () => {
    expect(imageType("/img/operator-help/chat.png")).toBe("image/png");
    expect(imageType("/img/logo.SVG")).toBe("image/svg+xml");
    expect(imageType("/img/readme.txt")).toBeNull();
  });
});

describe("the language of the documentation", () => {
  test("a mirrored page is asked and shown in the language of the application", async () => {
    // GIVEN an application in French
    const { deps, calls } = fakeShell();
    const reader = createDocumentationReader(deps);

    // WHEN a page that has a French mirror is opened
    await reader.start("fr-FR");
    await reader.open("/how-to/install-and-run");

    // THEN the shell is asked in that language and no mention is shown
    const state = get(reader);
    expect(calls.manifest).toEqual(["fr-FR"]);
    expect(calls.page.at(-1)).toEqual(["/how-to/install-and-run", "fr-FR"]);
    expect(state.page?.language).toBe("fr");
    expect(state.fallback).toBe(false);
  });

  test("a page without a mirror is shown in English with the mention", async () => {
    // GIVEN an application in French and a page written in English only
    const { deps } = fakeShell({ "/explanation/plugins": "# Plugins\n\nEnglish body.\n" });
    const reader = createDocumentationReader(deps);
    await reader.start("fr");

    // WHEN it is opened
    await reader.open("/explanation/plugins");

    // THEN the English source is shown and the pane carries the mention
    const state = get(reader);
    expect(state.page?.language).toBe("en");
    expect(state.view?.body).toBe("English body.");
    expect(state.fallback).toBe(true);
  });

  test("a generated reference is English without the mention", async () => {
    // GIVEN a reference generated from the code, English as on the site
    const { deps } = fakeShell();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");

    // WHEN it is opened
    await reader.open("/reference/sdk/llm");

    // THEN it is not marked as a missing translation
    expect(get(reader).page?.language).toBe("en");
    expect(get(reader).fallback).toBe(false);
  });

  test("a change of language reads the tree again and keeps the page", async () => {
    // GIVEN a page open in French
    const { deps, calls } = fakeShell();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");
    await reader.open("/how-to/audit-and-verify");

    // WHEN the application switches to English, then is told so twice
    await reader.start("en");
    await reader.start("en");

    // THEN the tree is English, the same page is read in English, once
    const state = get(reader);
    expect(calls.manifest).toEqual(["fr", "en"]);
    expect(state.manifest?.language).toBe("en");
    expect(state.route).toBe("/how-to/audit-and-verify");
    expect(calls.page.at(-1)).toEqual(["/how-to/audit-and-verify", "en"]);
  });
});

describe("Copy as Markdown", () => {
  test("copies the source of the page exactly as the site stores it", async () => {
    // GIVEN a page of the site with a front matter, an admonition and a
    // heading identifier, served as the archive holds it
    const source = siteFile("fr", "how-to/set-up-a-google-oauth-client.md");
    expect(source).toContain(":::");
    expect(source.startsWith("---\n")).toBe(true);
    const { deps, calls } = fakeShell({ "/how-to/install-and-run": source });
    const reader = createDocumentationReader(deps);
    await reader.start("fr");
    await reader.open("/how-to/install-and-run");

    // WHEN "Copy as Markdown" is used
    const copied = await reader.copyMarkdown();

    // THEN the clipboard holds the file of the site, byte for byte, not the
    // Markdown the pane renders
    expect(copied).toBe(true);
    expect(calls.copied).toEqual([source]);
    expect(get(reader).view?.body).not.toBe(source);
    expect(get(reader).copy).toBe("copied");
  });

  test("a clipboard that refuses is said, and the next page forgets it", async () => {
    // GIVEN a clipboard that refuses the write
    const { deps } = fakeShell();
    deps.writeClipboard = async () => {
      throw new Error("denied");
    };
    const reader = createDocumentationReader(deps);
    await reader.start("fr");

    // WHEN the page is copied, then another page opened
    expect(await reader.copyMarkdown()).toBe(false);
    expect(get(reader).copy).toBe("failed");
    await reader.open("/how-to/");

    // THEN the failure is not carried over
    expect(get(reader).copy).toBe("idle");
  });
});

describe("View on the web", () => {
  test("opens the route on the public site, under /fr/ in French", async () => {
    // GIVEN a page read in French, then the same in English
    const french = fakeShell();
    const frenchReader = createDocumentationReader(french.deps);
    await frenchReader.start("fr");
    await frenchReader.open("/how-to/install-and-run");
    const english = fakeShell();
    const englishReader = createDocumentationReader(english.deps);
    await englishReader.start("en");
    await englishReader.open("/how-to/install-and-run");

    // WHEN "View on the web" is used in each
    await frenchReader.viewOnWeb();
    await englishReader.viewOnWeb();

    // THEN the browser is handed the page of the site in the language read
    expect(french.calls.opened).toEqual(["https://docs.apollia.fr/fr/how-to/install-and-run"]);
    expect(english.calls.opened).toEqual(["https://docs.apollia.fr/how-to/install-and-run"]);
  });

  test("a page shown in English for want of a mirror still opens under /fr/", async () => {
    const { deps, calls } = fakeShell();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");
    await reader.open("/explanation/plugins");
    await reader.viewOnWeb();
    expect(calls.opened).toEqual(["https://docs.apollia.fr/fr/explanation/plugins"]);
  });

  test("a category list opens at its own route", async () => {
    const { deps, calls } = fakeShell();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");
    await reader.open("/reference/sdk");
    await reader.viewOnWeb();
    expect(calls.opened).toEqual(["https://docs.apollia.fr/fr/reference/sdk"]);
    expect(webAddress(fixtureManifest("en", PUBLIC_SITE), "reference/api")).toBe(
      "https://docs.apollia.fr/reference/api",
    );
  });
});

describe("the links of a page", () => {
  const manifest = fixtureManifest("fr", PUBLIC_SITE);

  test("a link to a page of the archive resolves to its route", () => {
    // GIVEN the three ways a page writes a link to another
    // WHEN each is resolved from the page that carries it
    // THEN each names the route, and the heading when one is given
    expect(resolveLink(manifest, "/", "/how-to/install-and-run")).toEqual({
      kind: "page",
      route: "/how-to/install-and-run",
      anchor: null,
    });
    expect(resolveLink(manifest, "/", "/how-to")).toEqual({
      kind: "page",
      route: "/how-to/",
      anchor: null,
    });
    expect(
      resolveLink(manifest, "/how-to/install-and-run", "./audit-and-verify.md#verify"),
    ).toEqual({ kind: "page", route: "/how-to/audit-and-verify", anchor: "verify" });
    expect(resolveLink(manifest, "/how-to/install-and-run", "../reference/sdk/llm.md")).toEqual({
      kind: "page",
      route: "/reference/sdk/llm",
      anchor: null,
    });
    expect(resolveLink(manifest, "/how-to/install-and-run", "/reference/sdk")).toEqual({
      kind: "page",
      route: "/reference/sdk",
      anchor: null,
    });
  });

  test("everything else leaves for the browser or goes nowhere", () => {
    expect(resolveLink(manifest, "/", "https://diataxis.fr")).toEqual({
      kind: "external",
      url: "https://diataxis.fr",
    });
    // A page of the site the archive does not hold opens on the public site.
    expect(resolveLink(manifest, "/", "/reference/api/apollia-os-runtime-api#x")).toEqual({
      kind: "external",
      url: "https://docs.apollia.fr/fr/reference/api/apollia-os-runtime-api#x",
    });
    expect(resolveLink(manifest, "/", "#start")).toEqual({ kind: "anchor", anchor: "start" });
    expect(resolveLink(manifest, "/", "javascript:alert(1)")).toEqual({ kind: "none" });
    expect(resolveLink(manifest, "/", "file:///etc/passwd")).toEqual({ kind: "none" });
    expect(resolveLink(manifest, "/", "//evil.example/x")).toEqual({ kind: "none" });
    expect(resolveLink(manifest, "/how-to/", "./gone.md")).toEqual({ kind: "none" });
    expect(resolveLink(manifest, "/", "  ")).toEqual({ kind: "none" });
  });

  test("an internal link opens the page in the pane, an external one the browser", async () => {
    // GIVEN a page open in the pane
    const { deps, calls } = fakeShell();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");
    await reader.open("/how-to/install-and-run");

    // WHEN an internal link of it is followed
    await reader.follow("./audit-and-verify.md#verify");

    // THEN the pane shows that page on that heading and the browser is not called
    expect(get(reader).route).toBe("/how-to/audit-and-verify");
    expect(get(reader).anchor).toBe("verify");
    expect(calls.opened).toEqual([]);

    // WHEN an external link is followed
    await reader.follow("https://diataxis.fr");

    // THEN the browser opens it and the pane stays on its page
    expect(calls.opened).toEqual(["https://diataxis.fr"]);
    expect(get(reader).route).toBe("/how-to/audit-and-verify");

    // WHEN a link to a heading of the page is followed
    await reader.follow("#summary");

    // THEN the page stays and the heading is the one to show
    expect(get(reader).route).toBe("/how-to/audit-and-verify");
    expect(get(reader).anchor).toBe("summary");
    expect(calls.opened).toEqual(["https://diataxis.fr"]);
  });
});

describe("a page opens with no network", () => {
  test("the reader reaches nothing but the three calls to the shell", async () => {
    // GIVEN a machine where any request to the network fails
    const fetched = vi.fn(async () => {
      throw new Error("network is down");
    });
    vi.stubGlobal("fetch", fetched);
    const { deps, calls } = fakeShell({
      "/how-to/install-and-run": "# Install\n\n![Chat](/img/operator-help/chat.png)\n",
    });
    const reader = createDocumentationReader(deps);

    // WHEN the documentation is read, a page opened and its image asked for
    await reader.start("fr");
    await reader.open("/how-to/install-and-run");
    const image = await reader.imageUrl("/img/operator-help/chat.png");
    const again = await reader.imageUrl("/img/operator-help/chat.png");

    // THEN the page and its image are there, the image read once, and
    // neither the network nor the browser was reached
    expect(get(reader).view?.title).toBe("Install");
    expect(get(reader).failed).toBe(false);
    expect(image).toBe("blob:image/png");
    expect(again).toBe(image);
    expect(calls.image).toEqual(["/img/operator-help/chat.png"]);
    expect(fetched).not.toHaveBeenCalled();
    expect(calls.opened).toEqual([]);
    vi.unstubAllGlobals();
  });

  test("an image the archive does not hold, or of another type, has no address", async () => {
    const { deps, calls } = fakeShell();
    const reader = createDocumentationReader(deps);
    expect(await reader.imageUrl("/img/missing.png")).toBeNull();
    expect(await reader.imageUrl("/img/notes.txt")).toBeNull();
    expect(calls.image).toEqual(["/img/missing.png"]);
  });

  test("a page the shell cannot read is said, and a later page clears it", async () => {
    // GIVEN a route the tree does not know
    const { deps } = fakeShell();
    const reader = createDocumentationReader(deps);
    await reader.start("fr");

    // WHEN it is opened, then a page that exists
    await reader.open("/nowhere");
    expect(get(reader).failed).toBe(true);
    await reader.open("/how-to/");

    // THEN the failure is gone
    expect(get(reader).failed).toBe(false);
    expect(get(reader).view?.title).toBe("/how-to/");
  });

  test("a slow page that answers after a newer one is dropped", async () => {
    // GIVEN a first page whose read is held back
    const { deps } = fakeShell();
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => (release = resolve));
    const page = deps.page;
    deps.page = async (route, language) => {
      if (route === "/how-to/install-and-run") await held;
      return page(route, language);
    };
    const reader = createDocumentationReader(deps);
    await reader.start("fr");

    // WHEN a second page is opened before the first answers
    const slow = reader.open("/how-to/install-and-run");
    await reader.open("/how-to/audit-and-verify");
    release();
    await slow;

    // THEN the pane shows the second
    expect(get(reader).route).toBe("/how-to/audit-and-verify");
    expect(get(reader).page?.route).toBe("/how-to/audit-and-verify");
  });
});
