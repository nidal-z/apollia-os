/**
 * Markdown renderer for chat content.
 *
 * Code blocks are emitted as *hydration placeholders*: the first render
 * returns the escaped source text, and `hydrateCodeBlocks` (driven from
 * `MarkdownContent.svelte`) replaces each block with a Shiki-highlighted
 * version once the highlighter finishes loading dynamically.
 *
 * This keeps the initial bundle small (Shiki's WASM + themes + languages are
 * loaded on demand), preserves first-paint, and respects `principle 1`
 * (no outbound traffic — Shiki bundles grammars locally via Vite).
 */
import { Marked } from "marked";
import DOMPurify from "dompurify";

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

const marked = new Marked({
  gfm: true,
  breaks: false,
  renderer: {
    code({ text, lang }) {
      const language = lang ?? "";
      const encodedCode = encodeURIComponent(text);
      const label = language
        ? `<span class="apollia-code-lang">${escapeHtml(language)}</span>`
        : "";
      // Escape the raw text for the first paint; the Shiki hydrator replaces
      // this span with syntax-highlighted markup asynchronously.
      const escaped = escapeHtml(text);
      return `<div class="apollia-code-block group/code">
        ${label}
        <button class="apollia-code-copy" data-copy-code data-code="${encodedCode}" title="Copy">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
        </button>
        <pre><code class="apollia-code-raw${language ? ` language-${escapeHtml(language)}` : ""}" data-shiki-lang="${escapeHtml(language)}" data-shiki-code="${encodedCode}">${escaped}</code></pre>
      </div>`;
    },

    link({ href, text }) {
      return `<a href="${escapeHtml(href ?? "")}" target="_blank" rel="noopener noreferrer">${text}</a>`;
    },
  },
});

const ALLOWED_TAGS = [
  "h1", "h2", "h3", "h4", "h5", "h6",
  "p", "br", "hr",
  "strong", "em", "del", "s",
  "ul", "ol", "li",
  "blockquote",
  "pre", "code", "span",
  "a",
  "table", "thead", "tbody", "tr", "th", "td",
  "div", "button", "svg", "rect", "path",
  "input",
];

const ALLOWED_ATTR = [
  "class", "href", "target", "rel", "title",
  "data-copy-code", "data-code",
  "data-shiki-lang", "data-shiki-code",
  "type", "checked", "disabled",
  // SVG attributes
  "width", "height", "viewBox", "fill", "stroke",
  "stroke-width", "stroke-linecap", "stroke-linejoin",
  "x", "y", "rx", "d",
];

export function renderMarkdown(raw: string): string {
  if (!raw) return "";

  const html = marked.parse(raw) as string;

  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
  });
}
