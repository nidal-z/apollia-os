/**
 * A page of the embedded documentation, made ready for the Guide tab.
 *
 * The source is the file of the documentation site, byte for byte: it opens
 * with a front matter and a first heading, and its links to other pages are
 * written from the root of the site. The tab shows the title itself, and a
 * link must leave for the public site in the browser, never be followed
 * inside the application.
 */
import type { DocumentationPage } from "$lib/ipc/documentation";

export interface DocumentationView {
  /** The first heading of the page. */
  title: string;
  /** The Markdown under that heading, its site links made absolute. */
  body: string;
}

const FRONT_MATTER = /^---\r?\n[\s\S]*?\r?\n---\r?\n/;
const FIRST_HEADING = /^#[ \t]+(.+?)[ \t]*\r?\n/;
/** A Markdown link whose target starts at the root of the site. */
const SITE_LINK = /\]\((\/[^)\s]*)\)/g;

/**
 * The public site in the language of `page`: its public address less its
 * route, which leaves the origin and, for a French page, its `/fr` prefix.
 */
export function documentationSiteRoot(page: DocumentationPage): string {
  return page.public_url.endsWith(page.route)
    ? page.public_url.slice(0, -page.route.length)
    : page.public_url;
}

/** The title and the body of `page`, as the Guide tab renders them. */
export function documentationView(page: DocumentationPage): DocumentationView {
  const source = page.markdown.replace(FRONT_MATTER, "").trimStart();
  const heading = FIRST_HEADING.exec(source);
  const root = documentationSiteRoot(page);
  const body = (heading ? source.slice(heading[0].length) : source)
    .trim()
    .replace(SITE_LINK, (_match, path: string) => `](${root}${path})`);
  return { title: heading ? heading[1] : "", body };
}
