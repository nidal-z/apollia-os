// The documentation reader of the developer screen: the page it shows, the
// tree it reads it from, and the gestures of the pane.
//
// Everything it shows comes from the archive compiled into the application,
// through three calls to the shell; the only address it ever opens is the
// public site, in the browser, when the user asks for it. The reader is a
// store so the tree of the column and the pane read the same page, and it
// takes its calls as arguments so Vitest plays it without a shell.

import { get, writable, type Readable } from "svelte/store";
import {
  getDocumentationImage,
  getDocumentationManifest,
  getDocumentationPage,
  type DocumentationManifest,
  type DocumentationPage,
} from "$lib/ipc/documentation";
import { openExternalUrl } from "$lib/utils/externalLink";
import {
  categoryIndex,
  firstRoute,
  hasSource,
  imageType,
  isMdx,
  preparePage,
  resolveLink,
  webAddress,
  type PreparedPage,
} from "./documentation";

/** What the reader needs from outside, replaceable in tests. */
export interface DocumentationReaderDeps {
  /** Reads the tree and the titles in a language. */
  manifest: (language: string) => Promise<DocumentationManifest>;
  /** Reads the source of a page in a language. */
  page: (route: string, language: string) => Promise<DocumentationPage>;
  /** Reads an image of the archive. */
  image: (path: string) => Promise<ArrayBuffer>;
  /** Opens an address in the browser of the system. */
  openUrl: (url: string) => Promise<void> | void;
  /** Writes text to the clipboard. */
  writeClipboard: (text: string) => Promise<void>;
  /** Makes an address the webview can show from the bytes of an image. */
  objectUrl: (bytes: ArrayBuffer, type: string) => string;
}

/** The outcome of the last "Copy as Markdown". */
export type CopyOutcome = "idle" | "copied" | "failed";

/** What the reader shows. */
export interface DocumentationReaderState {
  /** The language of the interface the documentation was asked in. */
  language: string;
  /** The tree and the titles, `null` until read. */
  manifest: DocumentationManifest | null;
  /** The route shown or being read, `null` before the first page. */
  route: string | null;
  /** The page as the shell served it, `null` for a category list or while reading. */
  page: DocumentationPage | null;
  /** The page ready to render, `null` while reading. */
  view: PreparedPage | null;
  /** The heading to bring into view once the page is rendered. */
  anchor: string | null;
  /** Set when the tree or the page could not be read. */
  failed: boolean;
  /** Set when the page is shown in English for want of a mirror. */
  fallback: boolean;
  /** The outcome of the last copy. */
  copy: CopyOutcome;
}

const EMPTY: DocumentationReaderState = {
  language: "",
  manifest: null,
  route: null,
  page: null,
  view: null,
  anchor: null,
  failed: false,
  fallback: false,
  copy: "idle",
};

/** The reader, as the tree and the pane use it. */
export interface DocumentationReader extends Readable<DocumentationReaderState> {
  /** Reads the documentation in `language`, and keeps the page already shown. */
  start: (language: string) => Promise<void>;
  /** Shows the page at `route`, on the heading `anchor` when one is named. */
  open: (route: string, anchor?: string | null) => Promise<void>;
  /** Follows a link written on the page shown. */
  follow: (href: string) => Promise<void>;
  /** Copies the source of the page shown, as the archive holds it. */
  copyMarkdown: () => Promise<boolean>;
  /** Opens the page shown on the public site. */
  viewOnWeb: () => Promise<void>;
  /** The address of the page shown on the public site, `null` before a page. */
  webUrl: () => string | null;
  /** An address the webview can show for an image of the archive, `null` when it has none. */
  imageUrl: (path: string) => Promise<string | null>;
}

/** A reader over `deps`. */
export function createDocumentationReader(deps: DocumentationReaderDeps): DocumentationReader {
  const store = writable<DocumentationReaderState>(EMPTY);
  // An image is read once: the archive is fixed for the life of the
  // application, and what the addresses hold is bounded by its images.
  const images = new Map<string, Promise<string | null>>();
  let reading = 0;

  async function open(route: string, anchor: string | null = null): Promise<void> {
    const { manifest, language } = get(store);
    if (!manifest) return;
    const turn = ++reading;
    store.update((s) => ({
      ...s,
      route,
      anchor,
      page: null,
      view: null,
      failed: false,
      fallback: false,
      copy: "idle",
    }));

    const title = manifest.navigation[route]?.title ?? "";
    if (!hasSource(manifest, route)) {
      const known = manifest.navigation[route] !== undefined;
      store.update((s) => ({
        ...s,
        view: known ? { title, body: categoryIndex(manifest.tree, route) } : null,
        failed: !known,
      }));
      return;
    }
    try {
      const page = await deps.page(route, language);
      if (turn !== reading) return;
      const view = preparePage(page, { mdx: isMdx(manifest, route), fallbackTitle: title });
      store.update((s) => ({ ...s, page, view, fallback: page.fallback }));
    } catch {
      if (turn === reading) store.update((s) => ({ ...s, failed: true }));
    }
  }

  async function start(language: string): Promise<void> {
    const before = get(store);
    if (before.manifest && before.language === language) return;
    const turn = ++reading;
    store.update((s) => ({ ...s, language, failed: false }));
    try {
      const manifest = await deps.manifest(language);
      if (turn !== reading) return;
      store.update((s) => ({ ...s, manifest }));
      const route = before.route ?? firstRoute(manifest.tree);
      if (route !== null) await open(route);
    } catch {
      if (turn === reading) store.update((s) => ({ ...s, failed: true }));
    }
  }

  function webUrl(): string | null {
    const { manifest, route, page } = get(store);
    if (page) return page.public_url;
    return manifest && route !== null ? webAddress(manifest, route) : null;
  }

  return {
    subscribe: store.subscribe,
    start,
    open,

    async follow(href: string): Promise<void> {
      const { manifest, route } = get(store);
      if (!manifest || route === null) return;
      const target = resolveLink(manifest, route, href);
      if (target.kind === "page") await open(target.route, target.anchor);
      else if (target.kind === "anchor") store.update((s) => ({ ...s, anchor: target.anchor }));
      else if (target.kind === "external") await deps.openUrl(target.url);
    },

    async copyMarkdown(): Promise<boolean> {
      const { page } = get(store);
      if (!page) return false;
      let copied = true;
      try {
        await deps.writeClipboard(page.markdown);
      } catch {
        copied = false;
      }
      store.update((s) => ({ ...s, copy: copied ? "copied" : "failed" }));
      return copied;
    },

    async viewOnWeb(): Promise<void> {
      const url = webUrl();
      if (url !== null) await deps.openUrl(url);
    },

    webUrl,

    imageUrl(path: string): Promise<string | null> {
      const type = imageType(path);
      if (type === null) return Promise.resolve(null);
      let known = images.get(path);
      if (!known) {
        known = deps.image(path).then(
          (bytes) => deps.objectUrl(bytes, type),
          () => null,
        );
        images.set(path, known);
      }
      return known;
    },
  };
}

/** The reader of the application, over the shell, the browser and the clipboard. */
export const documentationReader = createDocumentationReader({
  manifest: getDocumentationManifest,
  page: getDocumentationPage,
  image: getDocumentationImage,
  openUrl: openExternalUrl,
  writeClipboard: (text) => navigator.clipboard.writeText(text),
  objectUrl: (bytes, type) => URL.createObjectURL(new Blob([bytes], { type })),
});
