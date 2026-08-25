/**
 * Markdown renderer for chat content.
 *
 * The pipeline is: marked -> DOMPurify -> DOM decoration. `renderMarkdown`
 * returns sanitized markup that contains nothing but rendering tags, and the
 * interactive parts of a code block (wrapper, language label, copy button,
 * Shiki hydration attributes) are grafted onto the already-purified DOM by
 * `decorateCodeBlocks`, which every container that mounts this output calls.
 *
 * That ordering is the security boundary, not a styling detail. The renderer
 * used to emit the copy button itself, so `button`, `div`, `svg`, `input` and
 * the `class` / `data-*` attributes had to survive sanitization, and raw HTML
 * written by the model (or quoted from a page read by `web_read`) could ship
 * its own `<button data-copy-code data-code="...">`: the click handler then
 * copied a payload different from the text displayed, and a `class` of
 * bundled utilities like `fixed inset-0` could overlay the UI. With the
 * decoration applied after purification, `data-code` is always derived from
 * the block's own `textContent`, so what is copied is what is shown.
 *
 * Code blocks are emitted as *hydration placeholders*: the first render
 * returns the escaped source text, and `hydrateCodeBlocks` (driven from
 * `MarkdownContent.svelte`) replaces each block with a Shiki-highlighted
 * version once the highlighter finishes loading dynamically. This keeps the
 * initial bundle small (Shiki's WASM + themes + languages are loaded on
 * demand), preserves first-paint, and respects `principle 1` (no outbound
 * traffic - Shiki bundles grammars locally via Vite).
 */
import { Marked } from "marked";
import DOMPurify from "dompurify";
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { openExternalUrl, resolveExternalHref } from "$lib/utils/externalLink";

function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

const marked = new Marked({
  gfm: true,
  breaks: false,
  renderer: {
    code({ text, lang }) {
      const language = lang ?? "";
      const cls = language ? ` class="language-${escapeHtml(language)}"` : "";
      // Escape the raw text for the first paint; `decorateCodeBlocks` wraps
      // this placeholder and the Shiki hydrator highlights it asynchronously.
      return `<pre><code${cls}>${escapeHtml(text)}</code></pre>`;
    },

    link({ href, text }) {
      return `<a href="${escapeHtml(href ?? "")}" target="_blank" rel="noopener noreferrer">${text}</a>`;
    },
  },
});

/**
 * Rendering tags only. No structural or interactive tag (`div`, `button`,
 * `input`, `svg`) survives sanitization: the containers a code block needs
 * are added after purification by `decorateCodeBlocks`, so raw HTML from the
 * model cannot smuggle one in.
 */
const ALLOWED_TAGS = [
  "h1", "h2", "h3", "h4", "h5", "h6",
  "p", "br", "hr",
  "strong", "em", "del", "s",
  "ul", "ol", "li",
  "blockquote",
  "pre", "code",
  "a",
  "table", "thead", "tbody", "tr", "th", "td",
];

/**
 * No `data-*` attribute survives sanitization, and `class` only survives on
 * `<code>` when it is a fenced block's `language-...` marker (enforced by the
 * hook below). Everything the copy button and the Shiki hydrator read is set
 * programmatically after purification.
 */
const ALLOWED_ATTR = [
  "class", "href", "target", "rel", "title",
];

const CODE_LANGUAGE_CLASS = /^language-[A-Za-z0-9_+#.-]+$/;

// `class` stays in ALLOWED_ATTR only for the fenced-code language marker; on
// every other element, or with any other value, it is dropped here. A model
// that writes `<p class="fixed inset-0">` would otherwise pick up bundled
// Tailwind utilities and overlay the interface.
// Guarded: in the `node` Vitest environment DOMPurify is an inert stub with
// neither `addHook` nor `sanitize`, and the suites that import this module
// for its other exports must still load.
if (DOMPurify.isSupported) {
  DOMPurify.addHook("uponSanitizeAttribute", (node, data) => {
    if (data.attrName !== "class") return;
    if (node.tagName === "CODE" && CODE_LANGUAGE_CLASS.test(data.attrValue)) return;
    data.keepAttr = false;
  });
}

/**
 * Click delegation for the anchors the `link` renderer above injects.
 *
 * Those anchors reach the page through `{@html}`, so no Svelte component
 * owns them and `onclick={handleExternalLinkClick}` cannot be attached to
 * them one by one. Every container that renders `renderMarkdown` output has
 * to call this from its own click handler, before any other branch returns,
 * or the link is dead: the packaged webview ignores `target="_blank"`.
 *
 * Returns `true` when the click was routed to `openExternalUrl`, so the
 * caller can stop handling it.
 */
export function handleMarkdownLinkClick(event: MouseEvent): boolean {
  const origin = event.target as Element | null;
  const anchor = origin?.closest?.("a[href]") as HTMLAnchorElement | null;
  if (!anchor) return false;
  const href = resolveExternalHref(event, anchor);
  if (href === null) return false;
  event.preventDefault();
  void openExternalUrl(href);
  return true;
}

export function renderMarkdown(raw: string): string {
  if (!raw) return "";

  const html = marked.parse(raw) as string;

  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    // DOMPurify accepts every `data-*` attribute by default, over and above
    // ALLOWED_ATTR. Off, or a model-authored `data-copy-code` / `data-code`
    // pair survives sanitization and the copy button copies a payload the
    // user never saw.
    ALLOW_DATA_ATTR: false,
  });
}

const COPY_ICON =
  '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>';

/**
 * Graft the interactive code-block chrome onto already-sanitized DOM.
 *
 * Runs after `{@html renderMarkdown(...)}` has been mounted, so everything it
 * writes (`data-code`, `data-shiki-*`, the copy button) is derived from the
 * purified tree, never from the model's raw HTML. In particular `data-code`
 * is encoded from the block's own `textContent`: the copy button can only
 * ever copy the text the user sees. Idempotent, so streaming re-renders and
 * repeat effect passes are safe.
 */
export function decorateCodeBlocks(root: HTMLElement): void {
  const blocks = root.querySelectorAll<HTMLElement>("pre > code");
  for (const code of Array.from(blocks)) {
    if (code.closest(".apollia-code-block")) continue;
    const pre = code.parentElement;
    if (!pre?.parentElement) continue;

    const source = (code.textContent ?? "").replace(/\n$/, "");
    const language = CODE_LANGUAGE_CLASS.exec(code.className)?.[0]?.slice("language-".length) ?? "";
    const encoded = encodeURIComponent(source);

    const wrapper = document.createElement("div");
    wrapper.className = "apollia-code-block group/code";
    pre.before(wrapper);

    if (language) {
      const label = document.createElement("span");
      label.className = "apollia-code-lang";
      label.textContent = language;
      wrapper.append(label);
    }

    const copy = document.createElement("button");
    copy.className = "apollia-code-copy";
    copy.title = get(t)("common.copy");
    copy.setAttribute("data-copy-code", "");
    copy.setAttribute("data-code", encoded);
    copy.innerHTML = COPY_ICON;
    wrapper.append(copy);

    wrapper.append(pre);
    code.classList.add("apollia-code-raw");
    code.setAttribute("data-shiki-lang", language);
    code.setAttribute("data-shiki-code", encoded);
  }
}
