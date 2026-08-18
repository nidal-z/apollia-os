import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(async () => {}),
}));

import { openUrl } from "@tauri-apps/plugin-opener";
import { handleMarkdownLinkClick } from "$lib/utils/markdown";

/**
 * The anchors the markdown renderer injects reach the page through `{@html}`,
 * so no component owns them and no per-anchor `onclick` can be attached.
 * Their only route to `openExternalUrl`, and from there to the system
 * browser, is the click delegation exercised here. Both containers that
 * render markdown, `MarkdownContent.svelte` and `StreamingText.svelte`,
 * used to return on `if (!copyBtn) return;` before it ran. A click on a link in a chat answer,
 * in a reasoning card body or in a task detail did nothing at all.
 *
 * `renderMarkdown` itself is not called here: it ends in
 * `DOMPurify.sanitize`, which needs a DOM, and this suite runs in the `node`
 * environment the project configures. The anchor shape is read from the
 * renderer source instead, so the suite still fails if the `link` renderer
 * stops emitting the handler-less `target="_blank"` anchor it describes.
 */

const APP_ORIGIN = "http://localhost:1420";

const sources: Record<string, string> = import.meta.glob(
  [
    "/src/lib/utils/markdown.ts",
    "/src/lib/components/ui/markdown/MarkdownContent.svelte",
    "/src/components/chat/StreamingText.svelte",
  ],
  { query: "?raw", import: "default", eager: true },
);

const MARKDOWN_MODULE = "/src/lib/utils/markdown.ts";
const CONTAINERS = [
  "/src/lib/components/ui/markdown/MarkdownContent.svelte",
  "/src/components/chat/StreamingText.svelte",
];

/** The `link({ href, text })` body of the markdown renderer. */
function linkRendererBody(): string {
  const source = sources[MARKDOWN_MODULE];
  const body = /link\(\{ href, text \}\) \{([\s\S]*?)\n {4}\},/.exec(source);
  if (!body) throw new Error("link renderer not found in lib/utils/markdown.ts");
  return body[1];
}

function clickOn(href: string | null, modifiers: Partial<MouseEvent> = {}) {
  const anchor = href === null ? null : { href };
  const preventDefault = vi.fn();
  const event = {
    target: { closest: (selector: string) => (selector === "a[href]" ? anchor : null) },
    preventDefault,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    button: 0,
    ...modifiers,
  } as unknown as MouseEvent;
  return { event, preventDefault };
}

beforeEach(() => {
  vi.mocked(openUrl).mockClear();
  (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
  Object.defineProperty(globalThis, "location", {
    value: { origin: APP_ORIGIN },
    configurable: true,
    writable: true,
  });
});

afterEach(() => {
  delete (globalThis as { window?: unknown }).window;
});

describe("markdown link delegation", () => {
  it("emits anchors that carry no handler of their own", () => {
    // GIVEN the link renderer of lib/utils/markdown.ts
    const body = linkRendererBody();

    // WHEN its emitted anchor is inspected
    // THEN it opens a new context the packaged webview ignores, and it has no
    // per-anchor handler, so only a delegation can rescue the click
    expect(body).toContain('target="_blank"');
    expect(body).toContain("<a href=");
    expect(body).not.toContain("onclick");
  });

  it("routes a click on such an anchor to the opener instead of the default navigation", async () => {
    // GIVEN a click on a markdown anchor pointing outside the application
    const { event, preventDefault } = clickOn("https://docs.apollia.fr/start");

    // WHEN the delegation handles it
    const handled = handleMarkdownLinkClick(event);
    await Promise.resolve();

    // THEN the default navigation is cancelled and the opener plugin is called
    expect(handled).toBe(true);
    expect(preventDefault).toHaveBeenCalledOnce();
    expect(vi.mocked(openUrl)).toHaveBeenCalledWith("https://docs.apollia.fr/start");
  });

  it("leaves a click that is not on an anchor alone", async () => {
    // GIVEN a click that resolves to no anchor, on a code block for instance
    const { event, preventDefault } = clickOn(null);

    // WHEN the delegation handles it
    const handled = handleMarkdownLinkClick(event);
    await Promise.resolve();

    // THEN nothing is intercepted, so the copy-button branch still runs
    expect(handled).toBe(false);
    expect(preventDefault).not.toHaveBeenCalled();
    expect(vi.mocked(openUrl)).not.toHaveBeenCalled();
  });

  it("leaves in-app navigation alone", async () => {
    // GIVEN an anchor pointing at the application's own origin
    const { event, preventDefault } = clickOn(`${APP_ORIGIN}/settings`);

    // WHEN the delegation handles the click
    const handled = handleMarkdownLinkClick(event);
    await Promise.resolve();

    // THEN internal routing is not hijacked
    expect(handled).toBe(false);
    expect(preventDefault).not.toHaveBeenCalled();
    expect(vi.mocked(openUrl)).not.toHaveBeenCalled();
  });

  it.each(CONTAINERS)("calls the delegation before any other branch in %s", (container) => {
    // GIVEN the click handler of a container that renders markdown
    const source = sources[container];
    expect(source).toBeTypeOf("string");
    const delegation = source.indexOf("if (handleMarkdownLinkClick(event)) return;");
    const copyGuard = source.indexOf("if (!copyBtn) return;");

    // WHEN the order of its branches is read
    // THEN the delegation runs first: the copy-button branch returns on every
    // click that is not the copy button, anchor clicks included
    expect(delegation).toBeGreaterThan(-1);
    expect(copyGuard).toBeGreaterThan(-1);
    expect(delegation).toBeLessThan(copyGuard);
  });

  it.each(CONTAINERS)("hands the click to the opener from %s", async (container) => {
    // GIVEN a container whose handler starts with the delegation
    const source = sources[container];
    expect(source).toContain("if (handleMarkdownLinkClick(event)) return;");
    const { event, preventDefault } = clickOn("https://pypi.org/project/apollia");

    // WHEN a click lands on a markdown anchor inside it
    const handled = handleMarkdownLinkClick(event);
    await Promise.resolve();

    // THEN the container stops there and the opener gets the URL
    expect(handled).toBe(true);
    expect(preventDefault).toHaveBeenCalledOnce();
    expect(vi.mocked(openUrl)).toHaveBeenCalledWith("https://pypi.org/project/apollia");
  });
});
