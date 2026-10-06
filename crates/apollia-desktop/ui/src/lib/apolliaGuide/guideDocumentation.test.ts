import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  documentationSiteRoot,
  documentationView,
} from "$lib/apolliaGuide/guideDocumentation";
import type { DocumentationPage } from "$lib/ipc/documentation";

const ROUTE = "/operator-help/screens/agents";

/** The page of the Agents screen, as the site stores it. */
const SOURCE = readFileSync(
  fileURLToPath(new URL(`../../../../../../docs/site/docs${ROUTE}.md`, import.meta.url)),
  "utf8",
);

function page(markdown: string, language: "fr" | "en"): DocumentationPage {
  return {
    route: ROUTE,
    requested_language: language,
    language,
    fallback: false,
    generated: false,
    markdown,
    public_url: `https://docs.apollia.fr${language === "fr" ? "/fr" : ""}${ROUTE}`,
  };
}

describe("documentationView", () => {
  test("the page of the site is shown without its front matter, under its title", () => {
    // GIVEN the page of the Agents screen, byte for byte the site's file
    // WHEN it is made ready for the tab
    const view = documentationView(page(SOURCE, "en"));

    // THEN the title is its first heading, and the body opens under it
    expect(view.title).toBe("The Agents screen");
    expect(view.body).not.toContain("sidebar_position");
    expect(view.body).not.toContain("# The Agents screen");
    expect(view.body.startsWith("> Your agents and your workflows")).toBe(true);
    expect(view.body).toContain("## What you can do");
  });

  test("a link to another page leaves for the public site, in the language of the page", () => {
    // GIVEN the same page, whose links are written from the root of the site
    expect(SOURCE).toContain("](/operator-help/agents/install-an-agent)");

    // WHEN it is made ready, served as a French page then as an English one
    const french = documentationView(page(SOURCE, "fr"));
    const english = documentationView(page(SOURCE, "en"));

    // THEN each link is absolute, under the address of its language
    expect(french.body).toContain(
      "](https://docs.apollia.fr/fr/operator-help/agents/install-an-agent)",
    );
    expect(english.body).toContain(
      "](https://docs.apollia.fr/operator-help/agents/install-an-agent)",
    );
    expect(french.body).not.toMatch(/\]\(\//);
  });

  test("a page without front matter or heading is shown as it is", () => {
    // GIVEN a source that is plain text
    // WHEN it is made ready
    const view = documentationView(page("Plain words.\n", "en"));

    // THEN nothing is cut from it
    expect(view).toEqual({ title: "", body: "Plain words." });
  });

  test("the site root is the public address less the route", () => {
    // GIVEN a French and an English page
    // WHEN the root of the site is derived from each
    // THEN the French one carries its prefix
    expect(documentationSiteRoot(page("", "fr"))).toBe("https://docs.apollia.fr/fr");
    expect(documentationSiteRoot(page("", "en"))).toBe("https://docs.apollia.fr");
  });
});
