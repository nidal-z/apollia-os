// @vitest-environment jsdom
import { describe, test, expect, vi, beforeAll } from "vitest";
import { waitLocale } from "svelte-i18n";
import "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

import { DOC_IMAGE_ATTR, decorateCodeBlocks, renderMarkdown } from "$lib/utils/markdown";
import { preparePage } from "./documentation";
import {
  WEB_BUTTON_ATTR,
  decorateAdmonitions,
  decorateDiagrams,
  decorateHeadings,
  decorateImages,
} from "./documentationDecor";
import type { DocumentationPage } from "$lib/ipc/documentation";

// The copy button of a code block reads its tooltip from the catalogue.
beforeAll(async () => {
  await waitLocale();
});

/**
 * The pane renders a page through `renderMarkdown` in its documentation
 * profile, then decorates the sanitized DOM. DOMPurify does nothing without
 * a DOM, so this suite runs under jsdom, like the sanitizer suite of the chat.
 */
function mount(source: string, mdx = false): HTMLElement {
  const page: DocumentationPage = {
    route: "/how-to/install-and-run",
    requested_language: "en",
    language: "en",
    fallback: false,
    generated: false,
    markdown: source,
    public_url: "https://docs.example/how-to/install-and-run",
  };
  const view = preparePage(page, {
    mdx,
    fallbackTitle: "Title",
  });
  const root = document.createElement("div");
  root.innerHTML = renderMarkdown(view.body, "documentation");
  decorateCodeBlocks(root);
  return root;
}

describe("the rendered page", () => {
  test("is the Markdown of the source, rendered by the renderer of the interface", () => {
    // GIVEN a page with a table, a list, a link and code
    const root = mount(
      [
        "---",
        "title: x",
        "---",
        "# Title",
        "",
        "Intro with a [link](/how-to/audit-and-verify) and `code`.",
        "",
        "## What's new",
        "",
        "| Endpoint | Method |",
        "|---|---|",
        "| `/v1/models` | GET |",
        "",
        "```bash",
        "apollia-os start",
        "```",
      ].join("\n"),
    );

    // WHEN the headings are decorated
    decorateHeadings(root);

    // THEN the title is not repeated, and each element is there
    expect(root.querySelector("h1")).toBeNull();
    expect(root.querySelector("h2")?.id).toBe("whats-new");
    expect(root.querySelector("a")?.getAttribute("href")).toBe("/how-to/audit-and-verify");
    expect(root.querySelectorAll("table td")).toHaveLength(2);
    expect(root.querySelector(".apollia-code-block code")?.textContent).toContain(
      "apollia-os start",
    );
  });

  test("gives a heading the identifier the source writes, and numbers a repeated one", () => {
    const root = mount("# T\n\n## Setup {#custom-id}\n\n## Steps\n\n## Steps\n");
    decorateHeadings(root);
    decorateHeadings(root);
    const headings = Array.from(root.querySelectorAll("h2"));
    expect(headings.map((h) => h.id)).toEqual(["custom-id", "steps", "steps-1"]);
    expect(headings[0].textContent).toBe("Setup");
  });
});

describe("admonitions", () => {
  test("are rendered as boxes of their kind, with their title", () => {
    // GIVEN a page with a titled warning and an untitled note
    const root = mount(
      [
        "# T",
        "",
        ":::warning Reset Apollia entirely",
        "This removes **everything**.",
        ":::",
        "",
        ":::note",
        "Plain note.",
        ":::",
        "",
        "> An ordinary quote.",
      ].join("\n"),
    );

    // WHEN the sanitized page is decorated, twice
    const label = (kind: string) => `label:${kind}`;
    decorateAdmonitions(root, label);
    decorateAdmonitions(root, label);

    // THEN each admonition is a box of its kind, titled by the source or by
    // the name of the kind, its body rendered; the plain quote is left alone
    const boxes = Array.from(root.querySelectorAll(".apollia-admonition"));
    expect(boxes).toHaveLength(2);
    expect(boxes[0].classList.contains("apollia-admonition-warning")).toBe(true);
    expect(boxes[0].querySelector(".apollia-admonition-title")?.textContent).toBe(
      "Reset Apollia entirely",
    );
    expect(boxes[0].querySelector("strong")?.textContent).toBe("everything");
    expect(boxes[1].classList.contains("apollia-admonition-note")).toBe(true);
    expect(boxes[1].querySelector(".apollia-admonition-title")?.textContent).toBe("label:note");
    expect(root.textContent).not.toContain(":::");
    expect(root.textContent).not.toContain("[!");
    expect(root.querySelectorAll("blockquote")).toHaveLength(3);
  });

  test("the class of a box never comes from the source", () => {
    // GIVEN raw markup that claims to be an admonition
    const root = mount('# T\n\n<blockquote class="apollia-admonition fixed inset-0">x</blockquote>\n');

    // THEN the sanitizer dropped the class before any decoration
    expect(root.querySelector("blockquote")?.className).toBe("");
  });
});

describe("Mermaid diagrams", () => {
  test("are shown as code, with a button to the page on the web", () => {
    // GIVEN a page with a diagram and another code block
    const root = mount(
      ["# T", "", "```mermaid", "graph TD", "  A --> B", "```", "", "```bash", "ls", "```"].join("\n"),
    );

    // WHEN the page is decorated, twice
    decorateDiagrams(root, "View the diagram on the web");
    decorateDiagrams(root, "View the diagram on the web");

    // THEN the diagram is its source in a code block, nothing is drawn, and
    // one button under it leads to the web; the other block has none
    const diagram = root.querySelector("code.language-mermaid");
    expect(diagram?.textContent).toContain("A --> B");
    expect(root.querySelector("svg.mermaid, .mermaid svg")).toBeNull();
    const buttons = Array.from(root.querySelectorAll(`button[${WEB_BUTTON_ATTR}]`));
    expect(buttons).toHaveLength(1);
    expect(buttons[0].textContent).toBe("View the diagram on the web");
    expect(buttons[0].getAttribute("data-testid")).toBe("developer-docs-diagram-web");
    expect(diagram?.closest(".apollia-code-block")?.nextElementSibling).toBe(
      buttons[0].parentElement,
    );
  });
});

describe("MDX components", () => {
  test("are rendered as plain Markdown", () => {
    // GIVEN an MDX page with imports, a comment, a hero and link cards
    const root = mount(
      [
        "---",
        "slug: /",
        "---",
        "",
        "{/* a comment",
        "    on two lines */}",
        "",
        "import Link from '@docusaurus/Link';",
        "",
        '<div className="home-hero">',
        "  <img src={useBaseUrl('/img/logo.svg')} alt=\"\" />",
        "  <div>",
        "    <h1>Apollia OS documentation</h1>",
        "    <p>A sovereign runtime.</p>",
        "  </div>",
        "</div>",
        "",
        "## Start where you are",
        "",
        '<div className="home-grid">',
        '  <Link className="home-card" to="/tutorials">',
        "    <strong>Tutorials</strong>",
        "    <span>Learn by doing.</span>",
        "  </Link>",
        "</div>",
      ].join("\n"),
      true,
    );

    // THEN the text is there as paragraphs, a heading and a list of links,
    // and nothing of the components is shown or left as code
    expect(root.querySelector("p")?.textContent).toBe("A sovereign runtime.");
    expect(root.querySelector("h2")?.textContent).toBe("Start where you are");
    const card = root.querySelector("li a");
    expect(card?.getAttribute("href")).toBe("/tutorials");
    expect(card?.textContent).toBe("Tutorials");
    expect(root.querySelector("li")?.textContent).toContain("Learn by doing.");
    expect(root.querySelector("pre")).toBeNull();
    expect(root.textContent).not.toContain("import");
    expect(root.textContent).not.toContain("className");
    expect(root.textContent).not.toContain("comment");
  });
});

describe("images", () => {
  test("an image of the archive is named, and takes its source from the shell", async () => {
    // GIVEN a page with a screenshot of the archive and an image from elsewhere
    const root = mount(
      "# T\n\n![The chat](/img/operator-help/chat.png)\n\n![Remote](https://example.com/a.png)\n",
    );

    // THEN before decoration no image carries an address the webview could fetch
    const images = Array.from(root.querySelectorAll("img"));
    expect(images).toHaveLength(1);
    expect(images[0].hasAttribute("src")).toBe(false);
    expect(images[0].getAttribute("alt")).toBe("The chat");
    expect(images[0].getAttribute(DOC_IMAGE_ATTR)).toBe("/img/operator-help/chat.png");
    expect(root.textContent).toContain("Remote");

    // WHEN the page is decorated, twice
    const load = vi.fn(async (path: string) => `blob:${path}`);
    decorateImages(root, load);
    decorateImages(root, load);
    await Promise.resolve();
    await Promise.resolve();

    // THEN the image is asked for once and shows what the shell handed over
    expect(load).toHaveBeenCalledTimes(1);
    expect(images[0].getAttribute("src")).toBe("blob:/img/operator-help/chat.png");
  });

  test("raw markup cannot bring an image address into the page", () => {
    const root = mount('# T\n\n<img src="https://example.com/track.png" alt="x">\n');
    expect(root.querySelector("img")?.hasAttribute("src") ?? false).toBe(false);
  });

  test("the chat profile still renders no image at all", () => {
    const html = renderMarkdown("![The chat](/img/operator-help/chat.png)");
    expect(html).not.toContain("<img");
    expect(html).not.toContain(DOC_IMAGE_ATTR);
  });
});
