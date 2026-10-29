/**
 * The logo of a service of the selection, drawn from the application itself.
 *
 * Each logo is a Simple Icons file under `$lib/assets/brand-logos/`, copied
 * unmodified and listed with its source and its licence in `sources.json`
 * beside it, or, for a brand Simple Icons no longer carries, a mark drawn here
 * and listed there under `drawn` with the reason. Nothing is fetched to show one: the files are bundled, and the
 * glyph is the single path each of them draws on a 24 by 24 grid, filled with
 * the colour of the text around it so that it follows the theme. An entry
 * whose `icon_name` names no file keeps the generic icon.
 *
 * Pure, so the list, the add window and the card share one rule.
 */
import { deriveServerName, legacyServerName } from "$lib/integrations/connectorConfig";
import type { ProviderId } from "$lib/connections/capabilities";
import type { ConnectorEnrichmentView } from "$lib/types";

const FILES = import.meta.glob("../assets/brand-logos/*.svg", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** The path a Simple Icons file draws, or null when the file is not of that shape. */
export function svgPath(svg: string): string | null {
  const paths = [...svg.matchAll(/<path\b[^>]*\bd="([^"]+)"/g)];
  if (paths.length !== 1) return null;
  const d = paths[0][1];
  return /^[0-9A-Za-z .,\-]+$/.test(d) ? d : null;
}

const PATHS: ReadonlyMap<string, string> = new Map(
  Object.entries(FILES).flatMap(([file, svg]): [string, string][] => {
    const name = file.split("/").pop()?.replace(/\.svg$/, "");
    const d = svgPath(svg);
    return name && d ? [[name, d]] : [];
  }),
);

/** Every brand the interface carries a logo of, by its Simple Icons name. */
export function brandLogoNames(): string[] {
  return [...PATHS.keys()].sort();
}

/** The glyph of the logo `iconName` names, or null for a brand with none. */
export function brandLogoPath(iconName: string | null | undefined): string | null {
  return iconName ? (PATHS.get(iconName) ?? null) : null;
}

/**
 * The logo of each native account, by its file name. Microsoft's is drawn
 * here, since Simple Icons removed its logos at Microsoft's request, as
 * `sources.json` records under `drawn`.
 */
export const NATIVE_LOGOS: Readonly<Record<ProviderId, string | null>> = {
  google: "google",
  microsoft: "microsoft",
};

/** The glyph of the logo of a native account, or null for one with none. */
export function nativeLogoPath(provider: ProviderId): string | null {
  return brandLogoPath(NATIVE_LOGOS[provider]);
}

/**
 * The icon of the selection entry a server was added from, found by the name
 * the add window gives it (its first registry name, else its identifier) or
 * the name it was given before (its English label); null for a server that
 * comes from no entry.
 */
export function selectionIconFor(
  serverName: string | null,
  enrichments: ReadonlyMap<string, ConnectorEnrichmentView>,
): string | null {
  return selectionEntryFor(serverName, enrichments)?.icon_name ?? null;
}

/**
 * The selection entry a server was added from, found the same way as its
 * icon; null for a server that comes from no entry. It names a service by its
 * label rather than by the technical name of its server.
 */
export function selectionEntryFor(
  serverName: string | null,
  enrichments: ReadonlyMap<string, ConnectorEnrichmentView>,
): ConnectorEnrichmentView | null {
  if (serverName === null) return null;
  for (const [identifier, enrichment] of enrichments) {
    const name = enrichment.registry_names?.[0] ?? identifier;
    const view = { name, title: null, enrichment };
    if (deriveServerName(view) === serverName || legacyServerName(view) === serverName) {
      return enrichment;
    }
  }
  return null;
}

/** The logo name of an account of `provider`, or null for none (or no provider). */
export function nativeLogoName(provider: ProviderId | null): string | null {
  return provider ? NATIVE_LOGOS[provider] : null;
}

/**
 * The two letters that stand for a service with no logo, as "Ab": the first
 * letters of its first two words, else the first two letters of its one word.
 * Punctuation and a trailing domain ("monday.com", "Fireflies.ai") count for
 * nothing; a name with no letter or digit gives "?".
 */
export function serviceInitials(name: string): string {
  const words = name
    .replace(/\.(com|ai|io|app|dev|so|co)\b/gi, "")
    .split(/[^\p{L}\p{N}]+/u)
    .filter((word) => word.length > 0);
  if (words.length === 0) return "?";
  const [first, second] = words;
  const pair = second ? first[0] + second[0] : first.slice(0, 2);
  return pair.charAt(0).toLocaleUpperCase() + pair.slice(1).toLocaleLowerCase();
}
