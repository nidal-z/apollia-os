/**
 * Vendor / install-state helpers for catalogue entries.
 *
 * Pure and DOM-free. `installedNameFor` takes the pre-built index maps so it
 * stays a pure function (the caller owns the reactive derivation).
 */
import type { RegistryServerView } from "$lib/types";

/** Prettify a raw vendor slug: strip mcp/server noise, title-case the first word. */
export function prettyVendor(raw: string): string {
  const clean = raw
    .replace(/-mcp$|^mcp-|server[-_]?/gi, "")
    .replace(/[-_]+/g, " ")
    .trim();
  if (clean.length === 0) return raw;
  return clean.charAt(0).toUpperCase() + clean.slice(1);
}

/**
 * Derive a publisher/vendor label from a registry entry.
 *
 * Sources, in order of preference:
 *   1. `repository_url` host org segment (most reliable for community)
 *   2. `packages[0].identifier` namespace (`@notionhq/...`, reverse-DNS)
 *   3. Domain of a curated remote URL
 * Returns an empty string when nothing usable is found.
 */
export function vendorLabel(entry: RegistryServerView): string {
  const repoUrl = entry.repository_url;
  if (repoUrl) {
    try {
      const u = new URL(repoUrl);
      const segments = u.pathname.split("/").filter(Boolean);
      if (segments.length > 0) return prettyVendor(segments[0]);
    } catch {
      /* fall through */
    }
  }

  const pkgId = entry.packages?.[0]?.identifier ?? entry.name;
  if (pkgId.startsWith("@")) {
    const slash = pkgId.indexOf("/");
    if (slash > 1) return prettyVendor(pkgId.slice(1, slash));
  }
  const dot = pkgId.indexOf(".");
  const slash = pkgId.indexOf("/");
  if (dot > 0 && (slash === -1 || dot < slash)) {
    const after = pkgId.slice(dot + 1);
    const end =
      after.indexOf(".") >= 0
        ? after.indexOf(".")
        : after.indexOf("/") >= 0
          ? after.indexOf("/")
          : after.length;
    if (end > 0) return prettyVendor(after.slice(0, end));
  }

  const remoteUrl = entry.remotes?.[0]?.url;
  if (remoteUrl) {
    try {
      const u = new URL(remoteUrl);
      const parts = u.hostname.split(".").filter(Boolean);
      if (parts.length >= 2) return prettyVendor(parts[parts.length - 2]);
    } catch {
      /* fall through */
    }
  }
  return "";
}

/**
 * Resolve the installed server name (if any) for a registry catalogue entry.
 *
 * `installedNames` indexes installed servers by their sanitized name;
 * `installedByPackage` indexes them by their `package` field, bridging the
 * registry package identifier to the on-disk server name.
 */
export function installedNameFor(
  entry: RegistryServerView,
  installedNames: Set<string>,
  installedByPackage: Map<string, string>,
): string | null {
  if (installedNames.has(entry.name)) return entry.name;
  const byName = installedByPackage.get(entry.name);
  if (byName) return byName;
  const pkgId = entry.packages?.[0]?.identifier;
  if (pkgId) {
    const match = installedByPackage.get(pkgId);
    if (match) return match;
  }
  return null;
}

/** Whether a catalogue entry is already installed. */
export function isInstalled(
  entry: RegistryServerView,
  installedNames: Set<string>,
  installedByPackage: Map<string, string>,
): boolean {
  return entry.is_installed || installedNameFor(entry, installedNames, installedByPackage) !== null;
}

/** Whether an entry carries an "official" trust level. */
export function isOfficial(entry: RegistryServerView): boolean {
  return (
    entry.trust_level === "verified_official" ||
    entry.trust_level === "community_verified"
  );
}
