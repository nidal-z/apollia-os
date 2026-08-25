#!/usr/bin/env node
/*
 * Hardcoded UI string audit.
 *
 * Reads every `.svelte` and `.ts` file under `src/` and reports user-visible
 * copy that is not routed through the catalogue. Exit codes: 0 when the
 * enforced perimeter is clean, 1 when findings remain, 2 when the audit
 * could not measure anything (no file read, parser crash).
 *
 * The previous generation of this script grepped `.svelte` markup only,
 * blanked every `{...}` expression and `<script>` block, and required an
 * initial capital. That perimeter let through 158 English sentences in
 * `bashDescriber.ts`, French status labels in component scripts, template
 * literals, and symbol-opened texts ("· denied"). This version parses:
 *
 *   - markup with `svelte/compiler` (text nodes, copy-bearing attributes),
 *   - template expressions, `<script>` blocks and `.ts` modules with the
 *     TypeScript parser, deciding string literal by string literal from its
 *     syntactic context (property name, callee, comparison, type position),
 *
 * with no capitalization condition.
 *
 * A finding can be silenced when the string genuinely does not need
 * translation (brand name, technical id):
 *   - markup: `<!-- i18n-ignore: reason -->` on the line above (covers its
 *     line, the next line, and the rest of the element opening there),
 *   - script / .ts: `// i18n-ignore: reason` on the same line or the line
 *     above,
 *   - a region: `i18n-ignore-start: reason` ... `i18n-ignore-end`,
 *   - a whole file: `i18n-ignore-file: reason` anywhere in it (the file is
 *     then listed as excused in the coverage report).
 */
import { readFile, readdir, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { parse } from "svelte/compiler";
import ts from "typescript";

const ROOT = path.resolve(fileURLToPath(import.meta.url), "../../src");

/** Design-system showcase routes: findings counted, never enforced. */
const WHITELIST_FILES = new Set([
  "routes/Design.svelte",
  "routes/DesignMotion.svelte",
  "routes/DesignEmptyStates.svelte",
  "routes/DesignDarkMode.svelte",
  // Dev-only gestural automation runner, tree-shaken out of release builds;
  // its strings are report lines for `.apollia-automation/report.json`.
  "lib/automation/runner.ts",
]);

// Literal brand / technical tokens that don't translate.
const BRAND_WHITELIST = new Set([
  "Apollia",
  "Apollia OS",
  "Apollia Chat",
  "Apollia OS Runtime",
  "MCP",
  "GPU",
  "OpenAI",
  "Mistral",
  "Anthropic",
  "Ollama",
  "Metal",
  "CUDA",
  "Hugging Face",
  "Google Workspace",
  "Google Drive",
  "Microsoft 365",
  "Brave Search",
  "DuckDuckGo",
  "Visual Studio Code",
  "JetBrains Mono",
  "Inter Tight",
  "Apple Silicon",
  "Mac OS X",
  "llama.cpp",
  "whisper.cpp",
  "Google Calendar",
  "Google Docs",
  "Google Sheets",
  "Google Slides",
  "Google Forms",
  "Google Tasks",
  "Outlook Calendar",
  "NVIDIA GPU",
  // OAuth vocabulary kept as-is in both languages
  "Client ID",
  // SPDX license expression
  "MIT OR Apache-2.0",
  // placeholder example values for technical config fields
  "local-code",
  "qwen3-0.6b-q8_0",
  "gpt-4o-mini",
  "Qwen/Qwen3-30B-A3B-GGUF",
]);

/**
 * Tokens that never count as words of copy: unit and size abbreviations,
 * process-stream and key-cap names, identifiers shared by both languages.
 * Compared after stripping trailing punctuation, and case-sensitively, so a
 * sentence starting with "Total" still counts.
 */
const NON_COPY_TOKENS = new Set([
  "ms",
  "msg",
  "chars",
  "min",
  "sem",
  "px",
  "kB",
  "KB",
  "MB",
  "GB",
  "TB",
  "MiB",
  "GiB",
  "KiB",
  "Ko",
  "Mo",
  "Go",
  "To",
  "GHz",
  "MHz",
  "OS",
  "ID",
  "URL",
  "API",
  "JSON",
  "stdout",
  "stderr",
  "stdio",
  "total",
  "Ctrl",
  "Cmd",
  "Alt",
  "Shift",
  "Esc",
  "Tab",
]);

const IGNORE_DIRECTIVE = /\bi18n-ignore\b(?!-)/;
const IGNORE_START = /\bi18n-ignore-start\b/;
const IGNORE_END = /\bi18n-ignore-end\b/;
const IGNORE_FILE = /\bi18n-ignore-file\b/;

/** Shortest markup copy string the scanner reports, in characters. */
const MIN_COPY_LENGTH = 4;

/** Attributes / component props whose literal value is copy. */
const COPY_ATTRS = new Set([
  "title",
  "aria-label",
  "placeholder",
  "alt",
  "subtitle",
  "confirmLabel",
  "cancelLabel",
  "message",
  "label",
  "description",
  "tooltip",
  "heading",
]);

/**
 * Object property / variable names whose single-word literal value is copy
 * even without a space ("label: \"Toutes\"").
 */
const COPY_NAME_RE =
  /(label|title|message|body|heading|subtitle|caption|placeholder|tooltip|description|snippet)$/i;

/** Property / variable names whose value is never copy, whatever its shape. */
const NON_COPY_NAMES = new Set([
  "class",
  "className",
  "classes",
  "style",
  "id",
  "key",
  "keys",
  "keyStem",
  "labelKey",
  "descKey",
  "descriptionKey",
  "titleKey",
  "i18nKey",
  "outputSummaryKey",
  "testid",
  "testId",
  "testidPrefix",
  "data-testid",
  "icon",
  "variant",
  "size",
  "tone",
  "color",
  "href",
  "src",
  "url",
  "path",
  "dir",
  "cwd",
  "glob",
  "pattern",
  "regex",
  "format",
  "locale",
  "lang",
  "unit",
  "kind",
  "type",
  "status",
  "mode",
  "event",
  "event_type",
  "category",
  "accelerator",
  "shortcut",
  "command",
  "mime",
  "filename",
  "extension",
  "extensions",
  "name",
  "value",
  "field",
  "prop",
  "tag",
  "anchor",
  "route",
  "slug",
  "target",
  "rel",
  "method",
  "agent",
  "agent_id",
  "session_id",
  "model",
  "backend",
  "provider",
  "namespace",
  "role",
  "separator",
  "delimiter",
  "prefix",
  "suffix",
  "keywords",
  "size",
  "confirmWord",
]);

/** Callee names whose string arguments are never copy. */
const NON_COPY_CALLEES = new Set([
  "invoke",
  "emit",
  "listen",
  "once",
  "addEventListener",
  "removeEventListener",
  "dispatchEvent",
  "getItem",
  "setItem",
  "removeItem",
  "querySelector",
  "querySelectorAll",
  "getElementById",
  "closest",
  "matches",
  "matchMedia",
  "createElement",
  "setAttribute",
  "getAttribute",
  "hasAttribute",
  "removeAttribute",
  "RegExp",
  "URL",
  "URLSearchParams",
  "fetch",
  "open",
  "split",
  "join",
  "replace",
  "replaceAll",
  "startsWith",
  "endsWith",
  "includes",
  "indexOf",
  "lastIndexOf",
  "localeCompare",
  "normalize",
  "padStart",
  "padEnd",
  "warn",
  "error",
  "info",
  "debug",
  "log",
  "trace",
  "assert",
  "Error",
  "TypeError",
  "RangeError",
  "DateTimeFormat",
  "NumberFormat",
  "Intl",
  // class-list composers
  "cn",
  "clsx",
  "cva",
  "tv",
  "twMerge",
  "twJoin",
  // error plumbing: the message lands in an Error, not in the interface
  "withTimeout",
]);

/** i18n call shapes: every literal inside their arguments is a key or a
 * routed fallback, not hardcoded copy. */
const I18N_CALLEES = new Set(["$t", "t", "tr", "translate"]);

/** A dotted lowercase identifier is a catalogue key, not copy. */
const KEY_LIKE_RE = /^[a-z0-9_$-]+(\.[a-z0-9_$@{}-]+)+\.?$/;

/** A key or label prefix waiting for its interpolated tail: `settings.`. */
const KEY_PREFIX_RE = /^[\w$-]+[.:]$/;

/**
 * One whitespace-separated token that reads as a word of copy. A trailing
 * colon marks an identifier label (`chain:`, `ID:`), not a word.
 */
const WORD_TOKEN_RE = /^[\p{L}'’]{2,}[.,!?…]*$/u;

/** @param {string} text @returns {number} count of word-like tokens */
function wordTokens(text) {
  return text.split(/\s+/).filter((tok) => {
    if (tok.length === 0 || !WORD_TOKEN_RE.test(tok)) return false;
    const bare = tok.replace(/[.,!?…]+$/u, "");
    return !NON_COPY_TOKENS.has(bare) && !BRAND_WHITELIST.has(bare);
  }).length;
}

/**
 * True for a literal that is a CSS utility-class list: several tokens, most
 * of them hyphenated or pseudo-class prefixed (`focus-visible:ring-2`).
 *
 * @param {string} text
 * @returns {boolean}
 */
function looksLikeClassList(text) {
  const tokens = text.split(/\s+/).filter((tok) => tok.length > 0);
  if (tokens.length < 3) return false;
  const cssish = tokens.filter((tok) => /[-:/[\]]/.test(tok)).length;
  return cssish / tokens.length >= 0.4;
}

/** @param {string} raw @returns {string} whitespace-collapsed text */
function normalize(raw) {
  return raw.replaceAll(/\s+/g, " ").trim();
}

/**
 * 1-based line holding `index` in `source`.
 *
 * @param {string} source
 * @param {number} index
 * @returns {number}
 */
function lineOf(source, index) {
  let line = 1;
  for (let i = 0; i < index && i < source.length; i += 1) {
    if (source[i] === "\n") line += 1;
  }
  return line;
}

/**
 * Lines silenced by an `i18n-ignore` directive (markup or code form), 1-based.
 * A directive covers its own line, the line below it, and, when the next
 * `<` opens a tag, the rest of that opening tag (an attribute cannot carry a
 * comment of its own, and the tag may spread over a dozen lines).
 *
 * @param {string} src
 * @returns {Set<number>}
 */
function ignoredLines(src) {
  /** @type {Set<number>} */
  const silenced = new Set();
  const lines = src.split("\n");
  const lineStarts = [0];
  for (const line of lines) {
    lineStarts.push((lineStarts.at(-1) ?? 0) + line.length + 1);
  }

  // `i18n-ignore-start: reason` ... `i18n-ignore-end` silences the block.
  /** @type {number | null} */
  let blockStart = null;
  lines.forEach((line, index) => {
    if (IGNORE_START.test(line)) blockStart = index + 1;
    else if (IGNORE_END.test(line) && blockStart !== null) {
      for (let n = blockStart; n <= index + 1; n += 1) silenced.add(n);
      blockStart = null;
    }
  });

  lines.forEach((line, index) => {
    if (!IGNORE_DIRECTIVE.test(line)) return;
    silenced.add(index + 1);
    silenced.add(index + 2);

    const open = src.indexOf("<", lineStarts[index + 1] ?? src.length);
    if (open === -1) return;
    const close = endOfOpeningTag(src, open);
    if (close === -1) return;
    const first = index + 2;
    const last = lineOf(src, close);
    for (let n = first; n <= last; n += 1) silenced.add(n);
  });
  return silenced;
}

/**
 * Index of the `>` that closes the tag opening at `start`, quotes skipped.
 *
 * @param {string} src
 * @param {number} start
 * @returns {number}
 */
function endOfOpeningTag(src, start) {
  /** @type {string | null} */
  let quote = null;
  for (let i = start + 1; i < src.length; i += 1) {
    const c = src[i];
    if (quote !== null) {
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'") quote = c;
    else if (c === ">") return i;
    else if (c === "<") return -1;
  }
  return -1;
}

/** @typedef {{ kind: string, line: number, snippet: string }} Finding */

/* ────────────────────────── TypeScript scanner ────────────────────────── */

/**
 * Name of a callee expression: `foo(...)` → "foo", `a.b.warn(...)` → "warn",
 * `get(t)(...)` → "get(t)".
 *
 * @param {ts.Expression} callee
 * @returns {string}
 */
function calleeName(callee) {
  if (ts.isIdentifier(callee)) return callee.text;
  if (ts.isPropertyAccessExpression(callee)) return callee.name.text;
  if (
    ts.isCallExpression(callee) &&
    ts.isIdentifier(callee.expression) &&
    callee.expression.text === "get"
  ) {
    return "get(t)";
  }
  return "";
}

/**
 * Name under which a node's value is stored: the property name for
 * `label: "..."`, the variable name for `const statusLabel = ...`, the
 * left-hand side for `x.title = ...`. Walks through ternaries, template
 * spans, parentheses, arrays and `??` / `||` chains.
 *
 * @param {ts.Node} node
 * @returns {string}
 */
function bindingName(node) {
  let current = node;
  let parent = current.parent;
  while (parent !== undefined) {
    if (
      ts.isParenthesizedExpression(parent) ||
      ts.isConditionalExpression(parent) ||
      ts.isArrayLiteralExpression(parent) ||
      ts.isTemplateSpan(parent) ||
      ts.isTemplateExpression(parent) ||
      (ts.isBinaryExpression(parent) &&
        (parent.operatorToken.kind === ts.SyntaxKind.QuestionQuestionToken ||
          parent.operatorToken.kind === ts.SyntaxKind.BarBarToken))
    ) {
      current = parent;
      parent = current.parent;
      continue;
    }
    break;
  }
  if (parent === undefined) return "";
  if (ts.isPropertyAssignment(parent) && parent.initializer === current) {
    return ts.isIdentifier(parent.name) || ts.isStringLiteral(parent.name)
      ? parent.name.text
      : "";
  }
  if (ts.isVariableDeclaration(parent) && parent.initializer === current) {
    return ts.isIdentifier(parent.name) ? parent.name.text : "";
  }
  if (
    ts.isBinaryExpression(parent) &&
    parent.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
    parent.right === current
  ) {
    const left = parent.left;
    if (ts.isIdentifier(left)) return left.text;
    if (ts.isPropertyAccessExpression(left)) return left.name.text;
  }
  if (ts.isJsxAttribute?.(parent)) return String(parent.name.getText());
  return "";
}

/**
 * True when `node` sits in a context whose literals are never copy: an
 * import/export, a comparison, a `case`, a type position, a computed index,
 * a property *name*, a `throw` / `new Error`, a non-copy callee, an i18n
 * call, or a value stored under a non-copy name.
 *
 * @param {ts.Node} node
 * @returns {boolean}
 */
function inSkippedContext(node) {
  const stored = bindingName(node);
  if (
    stored !== "" &&
    (NON_COPY_NAMES.has(stored) ||
      /keys?$/i.test(stored) ||
      /class(es|name)?$/i.test(stored) ||
      stored.startsWith("data-"))
  ) {
    return true;
  }
  for (let p = node.parent; p !== undefined; p = p.parent) {
    if (ts.isImportDeclaration(p) || ts.isExportDeclaration(p)) return true;
    if (ts.isLiteralTypeNode(p) || ts.isTypeReferenceNode(p)) return true;
    if (ts.isCaseClause(p) && p.expression === node) return true;
    if (
      ts.isBinaryExpression(p) &&
      [
        ts.SyntaxKind.EqualsEqualsEqualsToken,
        ts.SyntaxKind.ExclamationEqualsEqualsToken,
        ts.SyntaxKind.EqualsEqualsToken,
        ts.SyntaxKind.ExclamationEqualsToken,
      ].includes(p.operatorToken.kind)
    ) {
      return true;
    }
    if (ts.isElementAccessExpression(p) && p.argumentExpression === node) {
      return true;
    }
    if (ts.isComputedPropertyName(p)) return true;
    if (ts.isPropertyAssignment(p) && p.name === node) return true;
    if (ts.isThrowStatement(p)) return true;
    if (ts.isNewExpression(p)) {
      const name = calleeName(p.expression);
      if (NON_COPY_CALLEES.has(name)) return true;
    }
    if (ts.isCallExpression(p)) {
      const name = calleeName(p.expression);
      if (I18N_CALLEES.has(name) || name === "get(t)") return true;
      if (NON_COPY_CALLEES.has(name)) return true;
    }
  }
  return false;
}

/**
 * True when a plain string literal reads as copy: at least two word-like
 * tokens, or one word-like token stored under a copy-bearing name.
 *
 * @param {string} text
 * @param {string} stored
 * @returns {boolean}
 */
function literalIsCopy(text, stored) {
  const value = normalize(text);
  if (value.length < 2) return false;
  if (BRAND_WHITELIST.has(value)) return false;
  if (KEY_LIKE_RE.test(value)) return false;
  if (looksLikeClassList(value)) return false;
  const words = wordTokens(value);
  if (words >= 2) return true;
  return words >= 1 && COPY_NAME_RE.test(stored) && !/keys?$/i.test(stored);
}

/**
 * True when a template literal reads as copy: any quasi holding two
 * word-like tokens, or one word-like token when the template interpolates
 * (the interpolation stands for the rest of the sentence).
 *
 * @param {string[]} quasis
 * @param {boolean} interpolates
 * @param {string} stored
 * @returns {boolean}
 */
function templateIsCopy(quasis, interpolates, stored) {
  let best = 0;
  for (const chunk of quasis) {
    const value = normalize(chunk);
    if (value.length === 0 || KEY_LIKE_RE.test(value)) continue;
    // `settings.${page}`: a bare identifier prefix, not the head of a phrase.
    if (KEY_PREFIX_RE.test(value)) continue;
    best = Math.max(best, wordTokens(value));
  }
  if (best >= 2) return true;
  if (best >= 1 && interpolates) return true;
  return best >= 1 && COPY_NAME_RE.test(stored);
}

/**
 * Scan a TypeScript source (a `.ts` module, a `<script>` block, or a
 * template expression) for hardcoded copy.
 *
 * @param {string} text - the code to scan
 * @param {string} source - the full file, for line numbers and directives
 * @param {number} offset - index of `text` inside `source`
 * @param {Set<number>} silenced
 * @returns {Finding[]}
 */
function scanCode(text, source, offset, silenced) {
  /** @type {Finding[]} */
  const findings = [];
  const file = ts.createSourceFile(
    "audit.ts",
    text,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );

  /** @param {ts.Node} node @param {string} snippet */
  function report(node, snippet) {
    const line = lineOf(source, offset + node.getStart(file));
    if (silenced.has(line)) return;
    findings.push({ kind: "code", line, snippet: normalize(snippet) });
  }

  /** @param {ts.Node} node */
  function visit(node) {
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
      if (!inSkippedContext(node) && literalIsCopy(node.text, bindingName(node))) {
        report(node, node.text);
      }
    } else if (ts.isTemplateExpression(node)) {
      if (!inSkippedContext(node)) {
        const quasis = [
          node.head.text,
          ...node.templateSpans.map((s) => s.literal.text),
        ];
        if (templateIsCopy(quasis, true, bindingName(node))) {
          report(node, quasis.join("${…}"));
        }
      }
      // Interpolated expressions may hold literals of their own.
      for (const span of node.templateSpans) visit(span.expression);
      return;
    }
    ts.forEachChild(node, visit);
  }

  visit(file);
  return findings;
}

/* ─────────────────────────── Svelte scanner ───────────────────────────── */

/**
 * Scan one `.svelte` source: markup text nodes, copy attributes, template
 * expressions, and `<script>` blocks.
 *
 * @param {string} source
 * @returns {Finding[]}
 */
function scanSvelteSource(source) {
  /** @type {Finding[]} */
  const findings = [];
  const silenced = ignoredLines(source);
  const ast = parse(source, { modern: true });

  /** @param {{start: number, end: number}} span */
  function expressionSpan(span) {
    const text = source.slice(span.start, span.end);
    findings.push(...scanCode(text, source, span.start, silenced));
  }

  /** @param {{ type: string, start: number, end: number, raw?: string, data?: string }} node
   *  @param {boolean} besideExpression */
  function textNode(node, besideExpression) {
    const raw = node.raw ?? node.data ?? "";
    const value = normalize(raw);
    if (value.length === 0) return;
    if (value.length < MIN_COPY_LENGTH && !besideExpression) return;
    if (wordTokens(value) < 1) return;
    if (BRAND_WHITELIST.has(value)) return;
    const anchor = node.start + (raw.length - raw.trimStart().length);
    const line = lineOf(source, anchor);
    if (silenced.has(line)) return;
    findings.push({ kind: "text", line, snippet: value });
  }

  /** @param {Record<string, unknown>} attr */
  function attribute(attr) {
    if (attr.type === "SpreadAttribute") {
      expressionSpan(/** @type {{start:number,end:number}} */ (attr.expression));
      return;
    }
    if (attr.type !== "Attribute") {
      // Directives: class:/style: never carry copy; on:/use: expressions may.
      const expr = /** @type {{start:number,end:number} | undefined} */ (
        attr.expression
      );
      if (
        expr &&
        typeof expr.start === "number" &&
        attr.type !== "ClassDirective" &&
        attr.type !== "StyleDirective" &&
        attr.type !== "AnimateDirective" &&
        attr.type !== "TransitionDirective"
      ) {
        expressionSpan(expr);
      }
      return;
    }
    const name = String(attr.name);
    const value = attr.value;
    if (value === true) return;
    const parts = Array.isArray(value) ? value : [value];
    for (const part of parts) {
      const node = /** @type {Record<string, unknown>} */ (part);
      if (node.type === "ExpressionTag") {
        if (name !== "class" && name !== "style" && !NON_COPY_NAMES.has(name)) {
          expressionSpan(
            /** @type {{start:number,end:number}} */ (node.expression),
          );
        }
        continue;
      }
      if (node.type !== "Text" || !COPY_ATTRS.has(name)) continue;
      const raw = String(node.raw ?? node.data ?? "");
      const text = normalize(raw);
      if (text.length < MIN_COPY_LENGTH && parts.length === 1) continue;
      if (wordTokens(text) < 1) continue;
      if (BRAND_WHITELIST.has(text)) continue;
      const line = lineOf(source, /** @type {number} */ (attr.start));
      if (silenced.has(line)) continue;
      findings.push({ kind: `attr:${name}`, line, snippet: text });
    }
  }

  /** @param {unknown} node */
  function walk(node) {
    if (Array.isArray(node)) {
      for (const child of node) walk(child);
      return;
    }
    if (typeof node !== "object" || node === null) return;
    const n = /** @type {Record<string, unknown>} */ (node);
    switch (n.type) {
      case "Text":
        // handled by the fragment pass below (needs sibling context)
        return;
      case "Attribute":
      case "SpreadAttribute":
      case "ClassDirective":
      case "StyleDirective":
      case "OnDirective":
      case "BindDirective":
      case "UseDirective":
      case "TransitionDirective":
      case "AnimateDirective":
      case "LetDirective":
        attribute(n);
        return;
      case "ExpressionTag":
      case "HtmlTag":
      case "RenderTag":
      case "ConstTag":
        if (n.expression) {
          expressionSpan(/** @type {{start:number,end:number}} */ (n.expression));
        }
        return;
      case "IfBlock":
        if (n.test) expressionSpan(/** @type {{start:number,end:number}} */ (n.test));
        walk(n.consequent);
        walk(n.alternate);
        return;
      case "EachBlock":
        if (n.expression) {
          expressionSpan(/** @type {{start:number,end:number}} */ (n.expression));
        }
        walk(n.body);
        walk(n.fallback);
        return;
      case "AwaitBlock":
        if (n.expression) {
          expressionSpan(/** @type {{start:number,end:number}} */ (n.expression));
        }
        walk(n.pending);
        walk(n.then);
        walk(n.catch);
        return;
      case "KeyBlock":
        if (n.expression) {
          expressionSpan(/** @type {{start:number,end:number}} */ (n.expression));
        }
        walk(n.fragment);
        return;
      case "Fragment": {
        const nodes = /** @type {Record<string, unknown>[]} */ (n.nodes ?? []);
        nodes.forEach((child, i) => {
          if (child.type === "Text") {
            const beside =
              nodes[i - 1]?.type === "ExpressionTag" ||
              nodes[i + 1]?.type === "ExpressionTag";
            textNode(
              /** @type {{type:string,start:number,end:number,raw?:string,data?:string}} */ (
                child
              ),
              beside,
            );
          } else {
            walk(child);
          }
        });
        return;
      }
      case "Comment":
      case "Script":
      case "SvelteOptions":
        return;
      default: {
        // Elements, components, snippet blocks: attributes + child fragment.
        const attrs = /** @type {unknown[] | undefined} */ (n.attributes);
        if (attrs) for (const a of attrs) walk(a);
        if (n.fragment) walk(n.fragment);
        if (n.body) walk(n.body);
        return;
      }
    }
  }

  walk(ast.fragment);

  // <script> blocks, scanned with the TypeScript parser.
  const scriptRe = /<script[^>]*>([\s\S]*?)<\/script>/g;
  let match;
  while ((match = scriptRe.exec(source)) !== null) {
    const start = match.index + match[0].indexOf(match[1]);
    findings.push(...scanCode(match[1], source, start, silenced));
  }

  return findings;
}

/**
 * Scan one `.ts` module.
 *
 * @param {string} source
 * @returns {Finding[]}
 */
function scanTsSource(source) {
  return scanCode(source, source, 0, ignoredLines(source));
}

/* ─────────────────────────────── Driver ───────────────────────────────── */

/**
 * @param {string} dir
 * @returns {AsyncGenerator<string>}
 */
async function* walkDir(dir) {
  for (const entry of await readdir(dir)) {
    const full = path.join(dir, entry);
    const info = await stat(full);
    if (info.isDirectory()) yield* walkDir(full);
    else if (entry.endsWith(".svelte")) yield full;
    else if (
      entry.endsWith(".ts") &&
      !entry.endsWith(".test.ts") &&
      !entry.endsWith(".d.ts")
    ) {
      yield full;
    }
  }
}

/**
 * Print what the run read, what it enforced, and what it excused.
 *
 * `enforced + excused.length` is the number of files walked, so a reader can
 * check the coverage against `find src -name '*.svelte' -o -name '*.ts'`
 * without trusting the guard's own arithmetic.
 *
 * @param {number} enforced
 * @param {{ file: string, count: number }[]} excused
 * @returns {void}
 */
function reportCoverage(enforced, excused) {
  const excusedFindings = excused.reduce((sum, e) => sum + e.count, 0);
  console.log(
    `audit-i18n: ${enforced + excused.length} files, ${enforced} enforced, ` +
      `${excused.length} excused (dev-only files and i18n-ignore-file directives)`,
  );
  for (const { file, count } of excused) {
    console.log(`  excused  ${file}  ${count} finding(s), not enforced`);
  }
  if (excused.length > 0) {
    console.log(`  excused findings, total: ${excusedFindings}`);
  }
}

async function main() {
  let totalFindings = 0;
  let enforced = 0;
  const offenders = [];
  /** @type {{ file: string, count: number }[]} */
  const excused = [];

  for await (const file of walkDir(ROOT)) {
    const rel = path.relative(ROOT, file);
    const raw = await readFile(file, "utf8");
    if (IGNORE_FILE.test(raw)) {
      excused.push({ file: `${rel} (i18n-ignore-file)`, count: 0 });
      continue;
    }
    /** @type {Finding[]} */
    let findings;
    try {
      findings = rel.endsWith(".svelte")
        ? scanSvelteSource(raw)
        : scanTsSource(raw);
    } catch (err) {
      console.error(`audit-i18n: failed to parse ${rel}: ${err}`);
      process.exit(2);
      return;
    }

    if (WHITELIST_FILES.has(rel)) {
      excused.push({ file: rel, count: findings.length });
      continue;
    }

    enforced += 1;
    if (findings.length === 0) continue;

    offenders.push({ file: rel, findings });
    totalFindings += findings.length;
  }

  if (enforced === 0) {
    console.error("audit-i18n: no file read under src/ - nothing measured");
    process.exit(2);
  }

  reportCoverage(enforced, excused);

  if (totalFindings === 0) {
    console.log(
      `✓ No hardcoded UI strings in the ${enforced} enforced files`,
    );
    process.exit(0);
  }

  console.error(`✗ Found ${totalFindings} hardcoded UI string(s):\n`);
  for (const { file, findings } of offenders) {
    console.error(`  ${file}`);
    for (const f of findings) {
      console.error(`    [${f.kind}:${f.line}] ${f.snippet}`);
    }
  }
  process.exit(1);
}

export { ignoredLines, scanCode, scanSvelteSource, scanTsSource };

const invokedDirectly =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  try {
    await main();
  } catch (err) {
    console.error(err);
    process.exit(2);
  }
}
