/**
 * The documentation embedded in the application: one page, read with no
 * network, in the language of the interface.
 */
import { invoke } from "@tauri-apps/api/core";

/** A page of the embedded documentation, as the shell serves it. */
export interface DocumentationPage {
  /** The route read, as the documentation site publishes it. */
  route: string;
  /** The language asked for. */
  requested_language: string;
  /** The language of `markdown`. */
  language: string;
  /** Set when the page has no mirror in the language asked for and is served in English. */
  fallback: boolean;
  /** Whether the page is generated from the code. */
  generated: boolean;
  /** The Markdown source of the page, as the site stores it. */
  markdown: string;
  /** The page on the public site, in the language asked for. */
  public_url: string;
}

/** Reads the page served at `route`, in the interface language `language`. */
export function getDocumentationPage(route: string, language: string): Promise<DocumentationPage> {
  return invoke<DocumentationPage>("get_documentation_page", { route, language });
}

/** A page the archive holds: its source file and the languages it exists in. */
export interface DocumentationPageInfo {
  /** The source file, relative to the documentation folder of the site. */
  source: string;
  /** The languages the page is written in. */
  languages: string[];
  /** Whether the page is generated from the code. */
  generated: boolean;
}

/** An entry of the sidebar tree: a page, or a category of entries. */
export type DocumentationTreeItem =
  | { kind: "doc"; route: string; label: string }
  | { kind: "category"; label: string; route: string | null; items: DocumentationTreeItem[] };

/** A neighbour of a page in reading order. */
export interface DocumentationNavLink {
  route: string;
  title: string;
}

/** The title of a page, with the page before it and the page after it. */
export interface DocumentationNavEntry {
  kind: string;
  title: string;
  previous: DocumentationNavLink | null;
  next: DocumentationNavLink | null;
}

/** The documentation as one language shows it. */
export interface DocumentationManifest {
  /** The language of the labels and the titles. */
  language: string;
  /** Every language the archive holds. */
  languages: string[];
  /** The entry of the archive that holds the OpenAPI document. */
  openapi: string;
  /** Every page, by route. */
  pages: Record<string, DocumentationPageInfo>;
  /** The public site in this language, without a trailing slash. */
  public_url: string;
  /** The sidebar tree. */
  tree: DocumentationTreeItem[];
  /** The title, the previous and the next of every page, by route. */
  navigation: Record<string, DocumentationNavEntry>;
}

/** Reads the tree and the titles, in the interface language `language`. */
export function getDocumentationManifest(language: string): Promise<DocumentationManifest> {
  return invoke<DocumentationManifest>("get_documentation_manifest", { language });
}

/**
 * Reads an image of the archive by `path`, the path a page writes it under
 * (such as `/img/operator-help/chat.png`). The answer is the file itself.
 */
export function getDocumentationImage(path: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("get_documentation_image", { path });
}

/**
 * Reads the OpenAPI document of the archive: the published contract of the
 * native API, which the reader renders as the HTTP reference.
 */
export function getDocumentationOpenapi(): Promise<unknown> {
  return invoke<unknown>("get_documentation_openapi");
}
