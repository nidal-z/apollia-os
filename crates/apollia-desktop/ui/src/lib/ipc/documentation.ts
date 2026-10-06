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
