// What the documentation pane grafts onto a rendered page, once the markup
// is sanitized: the identifiers of the headings, the admonitions, the note
// under a diagram, and the images of the archive.
//
// Everything here runs on purified DOM and is idempotent, like the
// decoration of the code blocks it follows.

import { DOC_IMAGE_ATTR } from "$lib/utils/markdown";
import { ADMONITION_MARK, headingSlug, type AdmonitionKind } from "./documentation";

/** The attribute of the button that opens the page on the public site. */
export const WEB_BUTTON_ATTR = "data-doc-web";

const EXPLICIT_ID = /[ \t]*\{#([A-Za-z0-9_-]+)\}[ \t]*$/;

/**
 * Gives every heading the identifier the site gives it, so a link to a
 * heading finds it: the one the source writes as `{#id}`, or the slug of its
 * text, numbered when the page repeats it.
 */
export function decorateHeadings(root: HTMLElement): void {
  const seen = new Map<string, number>();
  for (const heading of Array.from(root.querySelectorAll<HTMLElement>("h1,h2,h3,h4,h5,h6"))) {
    if (heading.id) {
      seen.set(heading.id, (seen.get(heading.id) ?? 0) + 1);
      continue;
    }
    const last = heading.lastChild;
    const explicit = last?.nodeType === 3 ? EXPLICIT_ID.exec(last.textContent ?? "") : null;
    if (explicit && last) {
      last.textContent = (last.textContent ?? "").replace(EXPLICIT_ID, "");
      heading.id = explicit[1];
      continue;
    }
    const slug = headingSlug(heading.textContent ?? "");
    if (slug === "") continue;
    const count = seen.get(slug) ?? 0;
    seen.set(slug, count + 1);
    heading.id = count === 0 ? slug : `${slug}-${count}`;
  }
}

/**
 * Turns the quotes that open on a `[!KIND] title` line into admonitions: the
 * kind becomes the class of the box, the line its title, `label(kind)` when
 * the source gives none.
 */
export function decorateAdmonitions(
  root: HTMLElement,
  label: (kind: AdmonitionKind) => string,
): void {
  for (const quote of Array.from(root.querySelectorAll<HTMLElement>("blockquote"))) {
    if (quote.classList.contains("apollia-admonition")) continue;
    const first = quote.firstElementChild;
    if (first?.tagName !== "P") continue;
    const mark = ADMONITION_MARK.exec((first.textContent ?? "").trim());
    if (!mark) continue;
    const kind = mark[1].toLowerCase() as AdmonitionKind;
    quote.classList.add("apollia-admonition", `apollia-admonition-${kind}`);
    quote.setAttribute("role", "note");
    first.classList.add("apollia-admonition-title");
    first.textContent = mark[2].trim() === "" ? label(kind) : mark[2].trim();
  }
}

/**
 * Leaves a diagram as the code it is written in, and puts under it the
 * button that opens the page on the public site, where it is drawn.
 */
export function decorateDiagrams(root: HTMLElement, label: string): void {
  for (const code of Array.from(root.querySelectorAll<HTMLElement>("code.language-mermaid"))) {
    const block = code.closest<HTMLElement>(".apollia-code-block") ?? code.parentElement;
    if (!block || block.nextElementSibling?.classList.contains("apollia-doc-diagram")) continue;
    const note = document.createElement("p");
    note.className = "apollia-doc-diagram";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "apollia-doc-diagram-link";
    button.textContent = label;
    button.setAttribute(WEB_BUTTON_ATTR, "");
    button.dataset.testid = "developer-docs-diagram-web";
    note.append(button);
    block.after(note);
  }
}

/**
 * Gives each image of the page its source: `load` answers an address the
 * webview can show for the path the page names, or `null` when the archive
 * has no such image, in which case the caption stays.
 */
export function decorateImages(
  root: HTMLElement,
  load: (path: string) => Promise<string | null>,
): void {
  for (const image of Array.from(root.querySelectorAll<HTMLImageElement>(`img[${DOC_IMAGE_ATTR}]`))) {
    if (image.hasAttribute("src") || image.hasAttribute("data-doc-image-asked")) continue;
    image.setAttribute("data-doc-image-asked", "");
    const path = image.getAttribute(DOC_IMAGE_ATTR) ?? "";
    void load(path).then((url) => {
      if (url !== null) image.src = url;
    });
  }
}

/** Brings the heading `anchor` into view, or the top of the page without one. */
export function revealAnchor(root: HTMLElement, anchor: string | null): void {
  const target = anchor === null ? null : root.ownerDocument.getElementById(anchor);
  if (target && root.contains(target)) target.scrollIntoView?.({ block: "start" });
  else root.closest<HTMLElement>("[data-doc-scroll]")?.scrollTo?.({ top: 0 });
}
