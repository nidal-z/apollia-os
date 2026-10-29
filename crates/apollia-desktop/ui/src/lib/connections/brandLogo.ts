/**
 * The logo of a service of the selection, drawn from the application itself.
 *
 * Each logo is a Simple Icons file under `$lib/assets/brand-logos/`, copied
 * unmodified and listed with its source and its licence in `sources.json`
 * beside it. Nothing is fetched to show one: the files are bundled, and the
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
 * The logo of each native account, by its Simple Icons name. Microsoft has
 * none: Simple Icons removed its logos at Microsoft's request, so the account
 * keeps the generic icon, as `sources.json` records under `not_carried`.
 */
export const NATIVE_LOGOS: Readonly<Record<ProviderId, string | null>> = {
  google: "google",
  microsoft: null,
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
