// The documentation pane of the developer screen: what its tree, its
// breadcrumb and its links decide, and how the source of a page of the site
// becomes Markdown the interface renders.
//
// The source is the file of the documentation site, byte for byte. It carries
// what the site understands and plain Markdown does not: a front matter,
// `:::` admonitions, heading identifiers, and, on one page, MDX components.
// The decisions live here, apart from the components, so Vitest can hold them
// without rendering anything.

import type {
  DocumentationManifest,
  DocumentationNavLink,
  DocumentationPage,
  DocumentationTreeItem,
} from "$lib/ipc/documentation";

// ── The tree ───────────────────────────────────────────────────────────────

/** A step of the breadcrumb: a category or the page, with its route if it has one. */
export interface BreadcrumbStep {
  label: string;
  route: string | null;
}

/** The first page of the tree in reading order, `null` for an empty tree. */
export function firstRoute(tree: readonly DocumentationTreeItem[]): string | null {
  for (const item of tree) {
    if (item.route) return item.route;
    if (item.kind === "category") {
      const inside = firstRoute(item.items);
      if (inside) return inside;
    }
  }
  return null;
}

/**
 * The way from the top of the tree down to `route`: the categories that hold
 * the page, then the page. Empty when the tree does not list the route.
 */
export function breadcrumbOf(
  tree: readonly DocumentationTreeItem[],
  route: string,
): BreadcrumbStep[] {
  for (const item of tree) {
    if (item.route === route) return [{ label: item.label, route }];
    if (item.kind !== "category") continue;
    const inside = breadcrumbOf(item.items, route);
    if (inside.length > 0) return [{ label: item.label, route: item.route }, ...inside];
  }
  return [];
}

/** The key of a category in the tree: its place, one index per level. */
export function categoryKey(parent: string, index: number): string {
  return parent === "" ? String(index) : `${parent}.${index}`;
}

/** The categories that hold `route`, by key: those the tree opens by itself. */
export function categoriesHolding(
  tree: readonly DocumentationTreeItem[],
  route: string | null,
  parent = "",
): string[] {
  if (route === null) return [];
  for (const [index, item] of tree.entries()) {
    if (item.kind !== "category") continue;
    const key = categoryKey(parent, index);
    const inside = categoriesHolding(item.items, route, key);
    if (inside.length > 0) return [key, ...inside];
    if (item.route === route || item.items.some((child) => child.route === route)) return [key];
  }
  return [];
}

/**
 * Whether a category shows its entries: the ones that hold the page read are
 * open, and a gesture of the user on a category overrides that.
 */
export function categoryOpen(
  key: string,
  holding: readonly string[],
  toggled: Readonly<Record<string, boolean>>,
): boolean {
  return toggled[key] ?? holding.includes(key);
}

/** The page before `route` and the page after it, in reading order. */
export function neighbours(
  manifest: DocumentationManifest,
  route: string,
): { previous: DocumentationNavLink | null; next: DocumentationNavLink | null } {
  const entry = manifest.navigation[route];
  return { previous: entry?.previous ?? null, next: entry?.next ?? null };
}

/** What names a category in its test id: its key, written with hyphens. */
export function categorySlug(key: string): string {
  return key.replaceAll(".", "-");
}

/** What names the tree entry of `route` in its test id. */
export function entrySlug(route: string): string {
  const slug = route.replace(/^\/+|\/+$/g, "").replace(/[^A-Za-z0-9]+/g, "-");
  return slug === "" ? "home" : slug;
}

/** The entries of the category served at `route`, `null` when no category is. */
export function categoryEntries(
  tree: readonly DocumentationTreeItem[],
  route: string,
): readonly DocumentationTreeItem[] | null {
  for (const item of tree) {
    if (item.kind !== "category") continue;
    if (item.route === route) return item.items;
    const inside = categoryEntries(item.items, route);
    if (inside) return inside;
  }
  return null;
}

/**
 * The page of a category the site writes no source for: the list of its
 * entries, as the site generates it.
 */
export function categoryIndex(tree: readonly DocumentationTreeItem[], route: string): string {
  return (categoryEntries(tree, route) ?? [])
    .filter((item) => item.route !== null)
    .map((item) => `- [${item.label}](${item.route})`)
    .join("\n");
}

/** Whether `route` is a page with a source, rather than a generated category list. */
export function hasSource(manifest: DocumentationManifest, route: string): boolean {
  return manifest.pages[route] !== undefined;
}

// ── The public site ────────────────────────────────────────────────────────

/** The address of `path` on the public site, in the language of the manifest. */
export function webAddress(manifest: DocumentationManifest, path: string): string {
  return `${manifest.public_url}${path.startsWith("/") ? path : `/${path}`}`;
}

// ── Links ──────────────────────────────────────────────────────────────────

/** Where a link of a page leads. */
export type LinkTarget =
  /** Another page of the archive: it opens in the pane. */
  | { kind: "page"; route: string; anchor: string | null }
  /** A heading of the page read. */
  | { kind: "anchor"; anchor: string }
  /** An address outside the archive: it opens in the browser. */
  | { kind: "external"; url: string }
  /** Nothing the reader follows. */
  | { kind: "none" };

const SCHEME = /^[a-z][a-z0-9+.-]*:/i;
const BROWSER_SCHEMES = /^(?:https?|mailto):/i;

function splitAnchor(href: string): [string, string | null] {
  const at = href.indexOf("#");
  if (at === -1) return [href, null];
  const anchor = href.slice(at + 1);
  return [href.slice(0, at), anchor === "" ? null : anchor];
}

/** `relative` resolved against the folder `base`, as path segments. */
function resolvePath(base: string, relative: string): string {
  const out = relative.startsWith("/") ? [] : base.split("/").filter(Boolean);
  for (const segment of relative.split("/")) {
    if (segment === "" || segment === ".") continue;
    if (segment === "..") out.pop();
    else out.push(segment);
  }
  return out.join("/");
}

function folderOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at === -1 ? "" : path.slice(0, at);
}

function routeOfSource(manifest: DocumentationManifest, source: string): string | null {
  for (const [route, info] of Object.entries(manifest.pages)) {
    if (info.source === source) return route;
  }
  return null;
}

/**
 * The route the reader opens for `path`, `null` when it holds nothing there.
 * The site serves some routes with a trailing slash and links to them without.
 */
export function knownRoute(manifest: DocumentationManifest, path: string): string | null {
  const bare = path.length > 1 ? path.replace(/\/+$/, "") : path;
  for (const candidate of [path, bare, `${bare}/`]) {
    if (manifest.navigation[candidate] || manifest.pages[candidate]) return candidate;
  }
  return null;
}

/**
 * Where the link `href`, written on the page at `route`, leads.
 *
 * A page writes its links three ways: from the root of the site
 * (`/how-to/x`), to a source file next to its own (`./x.md`), or to a full
 * address. The first two open in the pane when the reader holds the page;
 * a page of the site it does not hold (the page the site generates for one
 * operation of the HTTP reference) opens on the public site.
 */
export function resolveLink(
  manifest: DocumentationManifest,
  route: string,
  href: string,
): LinkTarget {
  const written = href.trim();
  if (written === "") return { kind: "none" };
  if (written.startsWith("#")) {
    const anchor = written.slice(1);
    return anchor === "" ? { kind: "none" } : { kind: "anchor", anchor };
  }
  if (SCHEME.test(written) || written.startsWith("//")) {
    return BROWSER_SCHEMES.test(written) ? { kind: "external", url: written } : { kind: "none" };
  }

  const [path, anchor] = splitAnchor(written);
  const suffix = anchor === null ? "" : `#${anchor}`;

  if (/\.mdx?$/i.test(path)) {
    const own = manifest.pages[route]?.source ?? "";
    const target = routeOfSource(manifest, resolvePath(folderOf(own), path));
    if (target !== null) return { kind: "page", route: target, anchor };
    return { kind: "none" };
  }

  const resolved = `/${resolvePath(folderOf(route), path)}`;
  const known = knownRoute(manifest, resolved);
  if (known !== null) return { kind: "page", route: known, anchor };
  return { kind: "external", url: `${webAddress(manifest, resolved)}${suffix}` };
}

// ── The source of a page, made renderable ──────────────────────────────────

/** A page ready for the pane. */
export interface PreparedPage {
  /** The first heading of the page, or its title in the tree. */
  title: string;
  /** The Markdown under that heading. */
  body: string;
}

const FRONT_MATTER = /^---\r?\n[\s\S]*?\r?\n---\r?\n/;
const FIRST_HEADING = /^#[ \t]+(.+?)[ \t]*(?:\r?\n|$)/;
const HEADING_ID = /[ \t]*\{#[A-Za-z0-9_-]+\}[ \t]*$/;
/** A fenced block or an inline code span: what a rewrite must leave alone. */
const CODE = /(^[ \t]*```[\s\S]*?^[ \t]*```[ \t]*$|^[ \t]*~~~[\s\S]*?^[ \t]*~~~[ \t]*$|`[^`\n]*`)/gm;

/** Applies `rewrite` to the text of `source` that is not code. */
function outsideCode(source: string, rewrite: (text: string) => string): string {
  return source
    .split(CODE)
    .map((part, index) => (index % 2 === 0 ? rewrite(part) : part))
    .join("");
}

function plainText(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * The Markdown an MDX page reads as once its components are gone: a link
 * card becomes a list entry, a heading and a paragraph written as tags
 * become Markdown, and the imports, the comments and the layout tags leave.
 */
export function mdxToMarkdown(source: string): string {
  const simplified = outsideCode(source, (text) =>
    text
      .replace(/\{\/\*[\s\S]*?\*\/\}/g, "")
      .replace(/^import\s.+\sfrom\s+['"][^'"]+['"];?[ \t]*$/gm, "")
      .replace(
        /^[ \t]*<Link\b[^>]*?\bto="([^"]+)"[^>]*>([\s\S]*?)<\/Link>[ \t]*$/gm,
        (_match, to: string, inner: string) => {
          const strong = /<strong>([\s\S]*?)<\/strong>/.exec(inner);
          if (!strong) return `- [${plainText(inner)}](${to})`;
          const rest = plainText(inner.replace(strong[0], ""));
          const entry = `- **[${plainText(strong[1])}](${to})**`;
          return rest === "" ? entry : `${entry}  \n  ${rest}`;
        },
      )
      .replace(
        /^[ \t]*<h([1-6])>([\s\S]*?)<\/h\1>[ \t]*$/gm,
        (_match, level: string, inner: string) =>
          `\n${"#".repeat(Number(level))} ${plainText(inner)}\n`,
      )
      .replace(/^[ \t]*<p>([\s\S]*?)<\/p>[ \t]*$/gm, (_match, inner: string) => `\n${inner.trim()}\n`)
      .replace(/^[ \t]*(?:<\/?[A-Za-z][^>\n]*>[ \t]*)+$/gm, "")
      .replace(/<\/?[A-Za-z][^>\n]*>/g, ""),
  );
  return simplified.replace(/\n{3,}/g, "\n\n");
}

/** The kinds of admonition the site writes. */
export const ADMONITION_KINDS = ["note", "tip", "info", "warning", "caution", "danger"] as const;
export type AdmonitionKind = (typeof ADMONITION_KINDS)[number];

/** The i18n key of the title an admonition takes when its source gives none. */
export const ADMONITION_LABELS: Record<AdmonitionKind, string> = {
  note: "dev.developer.docs.admonition.note",
  tip: "dev.developer.docs.admonition.tip",
  info: "dev.developer.docs.admonition.info",
  warning: "dev.developer.docs.admonition.warning",
  caution: "dev.developer.docs.admonition.caution",
  danger: "dev.developer.docs.admonition.danger",
};

const ADMONITION_OPEN = new RegExp(
  `^([ \\t]*):::(${ADMONITION_KINDS.join("|")})(?:[ \\t]+(.*?))?[ \\t]*$`,
);
const ADMONITION_CLOSE = /^[ \t]*:::[ \t]*$/;
const FENCE = /^[ \t]*(```|~~~)/;

/** The first line of an admonition once it is a quote: its kind and its title. */
export const ADMONITION_MARK = /^\[!(NOTE|TIP|INFO|WARNING|CAUTION|DANGER)\][ \t]*(.*)$/;

/**
 * Rewrites the `:::kind title` blocks of the site as quotes that open on a
 * `[!KIND] title` line. Plain Markdown renders a quote, and the pane turns
 * the marked ones into admonitions once the markup is sanitized.
 */
export function admonitionsToQuotes(source: string): string {
  const out: string[] = [];
  let fence: string | null = null;
  let indent: string | null = null;
  for (const line of source.split("\n")) {
    const fenced = FENCE.exec(line);
    if (fenced && fence === null) fence = fenced[1];
    else if (fenced && fenced[1] === fence) fence = null;

    if (indent === null) {
      const open = fence === null && !fenced ? ADMONITION_OPEN.exec(line) : null;
      if (!open) {
        out.push(line);
        continue;
      }
      indent = open[1];
      const title = (open[3] ?? "").trim();
      out.push(`${indent}> [!${open[2].toUpperCase()}]${title === "" ? "" : ` ${title}`}`, `${indent}>`);
      continue;
    }
    if (fence === null && !fenced && ADMONITION_CLOSE.test(line)) {
      indent = null;
      out.push("");
      continue;
    }
    const body = line.startsWith(indent) ? line.slice(indent.length) : line.trimStart();
    out.push(body === "" ? `${indent}>` : `${indent}> ${body}`);
  }
  return out.join("\n");
}

/**
 * The title and the body of `page`, as the pane renders them. `mdx` says the
 * source is an MDX file; `fallbackTitle` is the title the tree gives the page,
 * used when the source opens on no heading.
 */
export function preparePage(
  page: DocumentationPage,
  options: { mdx: boolean; fallbackTitle: string },
): PreparedPage {
  let source = page.markdown.replace(FRONT_MATTER, "");
  if (options.mdx) source = mdxToMarkdown(source);
  source = admonitionsToQuotes(source).trimStart();
  const heading = FIRST_HEADING.exec(source);
  if (!heading) return { title: options.fallbackTitle, body: source.trim() };
  return {
    title: heading[1].replace(HEADING_ID, ""),
    body: source.slice(heading[0].length).trim(),
  };
}

/** Whether the source of `route` is an MDX file. */
export function isMdx(manifest: DocumentationManifest | null, route: string): boolean {
  return /\.mdx$/i.test(manifest?.pages[route]?.source ?? "");
}

// ── Headings ───────────────────────────────────────────────────────────────

/**
 * The identifier the site gives a heading from its text: lower case, the
 * punctuation gone, the spaces turned into hyphens.
 */
export function headingSlug(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\p{M}\s_-]/gu, "")
    .replace(/\s/g, "-");
}

// ── Images ─────────────────────────────────────────────────────────────────

const IMAGE_TYPES: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  svg: "image/svg+xml",
};

/** The media type of an image of the archive, from its extension. */
export function imageType(path: string): string | null {
  const extension = /\.([A-Za-z0-9]+)$/.exec(path)?.[1].toLowerCase() ?? "";
  return IMAGE_TYPES[extension] ?? null;
}
