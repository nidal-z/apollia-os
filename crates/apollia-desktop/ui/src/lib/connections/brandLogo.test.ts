import { describe, expect, test } from "vitest";
import { readdirSync, readFileSync } from "node:fs";

import {
  NATIVE_LOGOS,
  brandLogoNames,
  brandLogoPath,
  nativeLogoName,
  nativeLogoPath,
  selectionEntryFor,
  selectionIconFor,
  svgPath,
} from "$lib/connections/brandLogo";
import type { ConnectorEnrichmentView } from "$lib/types";

const LOGOS = new URL("../assets/brand-logos/", import.meta.url);
const logo = (file: string): URL => new URL(file, LOGOS);

interface SelectionEntry {
  package_identifier: string;
  registry_names?: string[];
  operator_label: Record<string, string>;
  icon_name: string;
}

interface LogoSource {
  icon_name: string;
  file: string;
  title: string;
  source: string;
  license: string;
  attribution?: string;
}

const selection = JSON.parse(readFileSync(new URL("../../../../src/mcp/enrichments.json", import.meta.url), "utf8")) as SelectionEntry[];
const sources = JSON.parse(readFileSync(logo("sources.json"), "utf8")) as {
  simple_icons_version: string;
  logos: LogoSource[];
  not_carried?: { brand: string; used_by: string; reason: string }[];
};
const svgFiles = readdirSync(LOGOS).filter((f) => f.endsWith(".svg")).sort();

/** The icons of the selection with no logo: a generic name, or a brand Simple Icons does not carry. */
const GENERIC = [
  "activity",
  "book-open",
  "bug",
  "chart",
  "file-text",
  "globe",
  "kanban",
  "landmark",
  "mail",
  "mic",
  "palette",
  "search",
  "slack",
  "users",
];

function enrichmentOf(entry: SelectionEntry): ConnectorEnrichmentView {
  return {
    operator_label: entry.operator_label,
    category: "data",
    icon_name: entry.icon_name,
    registry_names: entry.registry_names,
    trust_level: "verified_official",
    auth_help_url: null,
    auth_help_text: null,
    auth_help_i18n_key: null,
    oauth_pre_registered_client_id_env: null,
  };
}

describe("brand logos", () => {
  test("every logo file is listed with its Simple Icons source and licence, and every listing has its file", () => {
    // GIVEN the logo files and the list that declares them
    // WHEN both are read
    const listed = sources.logos.map((l) => l.file).sort();
    // THEN they name the same files, each from the pinned Simple Icons tag,
    //      under CC0 or, for a brand with its own licence, with its attribution
    expect(listed).toEqual(svgFiles);
    for (const logo of sources.logos) {
      expect(logo.file).toBe(`${logo.icon_name}.svg`);
      expect(logo.source).toBe(
        `https://github.com/simple-icons/simple-icons/blob/${sources.simple_icons_version}/icons/${logo.file}`,
      );
      if (logo.license !== "CC0-1.0") expect(logo.attribution ?? "").not.toBe("");
    }
  });

  test("every logo file draws one path the renderer reads", () => {
    // GIVEN each logo file as it was copied
    for (const file of svgFiles) {
      // WHEN its path is read
      const d = svgPath(readFileSync(logo(file), "utf8"));
      // THEN there is one, and the bundled map serves it under its name
      expect(d, file).not.toBeNull();
      expect(brandLogoPath(file.replace(/\.svg$/, ""))).toBe(d);
    }
    expect(brandLogoNames()).toEqual(svgFiles.map((f) => f.replace(/\.svg$/, "")));
  });

  test("every entry of the selection has its logo, or a generic icon Simple Icons leaves no choice of", () => {
    // GIVEN the selection compiled into the application
    // WHEN the icon of each entry is resolved
    const missing = selection
      .filter((e) => brandLogoPath(e.icon_name) === null)
      .map((e) => e.icon_name);
    // THEN only the declared generic names have no logo, and every logo shipped
    //      is used by an entry or by an account of a native provider
    expect([...new Set(missing)].sort()).toEqual(GENERIC);
    const used = new Set([...selection.map((e) => e.icon_name), ...Object.values(NATIVE_LOGOS)]);
    expect(brandLogoNames().filter((n) => !used.has(n))).toEqual([]);
  });

  test("the Google account has its logo, and Microsoft's generic icon is a choice written down", () => {
    // GIVEN the native accounts, no provider, and the list of logo sources
    // WHEN the logo of each account is resolved, by its name and by its glyph
    const google = nativeLogoPath("google");
    const microsoft = nativeLogoPath("microsoft");
    // THEN Google names and draws the Simple Icons file of the pinned tag,
    //      Microsoft and no provider have none, and sources.json says why
    expect(nativeLogoName("google")).toBe("google");
    expect(brandLogoPath(nativeLogoName("google"))).toBe(google);
    expect(nativeLogoName("microsoft")).toBeNull();
    expect(nativeLogoName(null)).toBeNull();
    expect(google).not.toBeNull();
    expect(google).toBe(svgPath(readFileSync(logo("google.svg"), "utf8")));
    expect(sources.logos.find((l) => l.icon_name === "google")?.source).toBe(
      "https://github.com/simple-icons/simple-icons/blob/16.35.0/icons/google.svg",
    );
    expect(microsoft).toBeNull();
    const written = sources.not_carried?.find((n) => n.brand === "Microsoft");
    expect(written?.reason ?? "").toContain("Simple Icons");
  });

  test("a path that is not the single path of a Simple Icons file is refused", () => {
    // GIVEN a file with two paths and one whose path carries markup
    const two = '<svg><path d="M0 0h1"/><path d="M1 1h1"/></svg>';
    const markup = '<svg><path d="M0 0&quot;h1"/></svg>';
    // WHEN their path is read
    // THEN neither is drawn, and an unknown name has no logo
    expect(svgPath(two)).toBeNull();
    expect(svgPath(markup)).toBeNull();
    expect(brandLogoPath("not-a-brand")).toBeNull();
    expect(brandLogoPath(null)).toBeNull();
  });

  test("a server takes the icon of the entry it was added from, by its name or its former name", () => {
    // GIVEN the selection keyed as the page reads it
    const map = new Map(selection.map((e) => [e.package_identifier, enrichmentOf(e)]));
    // WHEN servers named as the add window names them are looked up
    // THEN each finds the icon of its entry, and a custom server finds none
    expect(selectionIconFor("com-paypal-mcp-mcp", map)).toBe("paypal");
    expect(selectionIconFor("com-dropbox-mcp", map)).toBe("dropbox");
    expect(selectionIconFor("com-notion-mcp", map)).toBe("notion");
    expect(selectionIconFor("stripe", map)).toBe("stripe");
    expect(selectionIconFor("my-own-server", map)).toBeNull();
    expect(selectionIconFor(null, map)).toBeNull();
  });

  test("a server finds the entry it was added from, so it is named by its label", () => {
    // GIVEN the selection keyed as the page reads it
    const map = new Map(selection.map((e) => [e.package_identifier, enrichmentOf(e)]));
    // WHEN the entry of a server named by the add window is looked up
    // THEN it is the entry whose label names the service, and a custom server has none
    expect(selectionEntryFor("com-notion-mcp", map)?.operator_label.en).toBe("Notion");
    expect(selectionEntryFor("my-own-server", map)).toBeNull();
  });
});
