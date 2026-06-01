/**
 * Shiki-based syntax-highlight hydrator.
 *
 * `renderMarkdown()` emits every code block as a hydration placeholder:
 *   <pre><code class="apollia-code-raw" data-shiki-lang="ts" data-shiki-code="…">escaped</code></pre>
 *
 * `hydrateCodeBlocks(root)` walks the DOM subtree, lazily loads Shiki once,
 * and swaps each placeholder's inner HTML with a highlighted version.
 * Blocks that have already been hydrated (class `apollia-code-hi`) are
 * skipped so repeat calls (streaming, re-renders) are idempotent.
 */
import type { Highlighter, BundledLanguage } from "shiki";

const SUPPORTED_LANGS: readonly BundledLanguage[] = [
  "ts",
  "tsx",
  "js",
  "jsx",
  "python",
  "rust",
  "bash",
  "json",
  "html",
  "css",
  "sql",
  "yaml",
  "toml",
  "md",
] as const;

let highlighterPromise: Promise<Highlighter> | null = null;

async function getHighlighter(): Promise<Highlighter> {
  if (highlighterPromise !== null) return highlighterPromise;
  highlighterPromise = (async () => {
    const { createHighlighter } = await import("shiki");
    return createHighlighter({
      themes: ["github-light", "github-dark"],
      langs: SUPPORTED_LANGS as BundledLanguage[],
    });
  })();
  return highlighterPromise;
}

function isBundledLanguage(s: string): s is BundledLanguage {
  return (SUPPORTED_LANGS as readonly string[]).includes(s);
}

function normaliseLang(raw: string): BundledLanguage | null {
  const lower = raw.toLowerCase();
  const map: Record<string, BundledLanguage> = {
    typescript: "ts",
    javascript: "js",
    sh: "bash",
    shell: "bash",
    yml: "yaml",
    markdown: "md",
  };
  const resolved = map[lower] ?? lower;
  return isBundledLanguage(resolved) ? resolved : null;
}

function detectTheme(): "github-light" | "github-dark" {
  if (typeof document === "undefined") return "github-light";
  return document.documentElement.classList.contains("dark")
    ? "github-dark"
    : "github-light";
}

/**
 * Hydrate every unresolved code placeholder under `root`.  Safe to call
 * repeatedly - already-hydrated blocks are skipped.
 */
export async function hydrateCodeBlocks(root: HTMLElement): Promise<void> {
  const blocks = Array.from(
    root.querySelectorAll<HTMLElement>("code.apollia-code-raw"),
  ).filter((el) => !el.classList.contains("apollia-code-hi"));

  if (blocks.length === 0) return;

  let highlighter: Highlighter;
  try {
    highlighter = await getHighlighter();
  } catch {
    // Shiki failed to load - leave the escaped placeholder as-is.
    return;
  }

  const theme = detectTheme();

  for (const code of blocks) {
    const encodedSource = code.dataset.shikiCode ?? "";
    const rawLang = code.dataset.shikiLang ?? "";
    const source = (() => {
      try {
        return decodeURIComponent(encodedSource);
      } catch {
        return code.textContent ?? "";
      }
    })();
    const lang = normaliseLang(rawLang);

    let highlighted: string;
    try {
      highlighted = highlighter.codeToHtml(source, {
        lang: lang ?? "text",
        theme,
      });
    } catch {
      continue;
    }

    // Shiki wraps the code in its own <pre><code> - we only want the inner
    // <code> spans so we preserve our outer .apollia-code-block structure.
    const parser = new DOMParser();
    const doc = parser.parseFromString(highlighted, "text/html");
    const shikiCode = doc.querySelector("code");
    if (shikiCode === null) continue;

    code.innerHTML = shikiCode.innerHTML;
    code.classList.add("apollia-code-hi", `shiki-theme-${theme.includes("dark") ? "dark" : "light"}`);
  }
}
