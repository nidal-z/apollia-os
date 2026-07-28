/**
 * accentTokenFor - deterministic, theme-aware accent for a connector tile.
 *
 * Replaces the former hardcoded hex palette (`logoColorFor`, hex array): the
 * name hash selects one of the five semantic accent tokens, so the colour
 * stays stable per connector yet resolves through the design system in both
 * light and dark themes. No raw palette, no hex.
 */

/** Semantic accent tokens, resolved via `hsl(var(--token))` at render time. */
const ACCENT_TOKENS = [
  "hsl(var(--primary))",
  "hsl(var(--secondary))",
  "hsl(var(--info))",
  "hsl(var(--success))",
  "hsl(var(--warning))",
] as const;

/** Stable index derived from the connector name (FNV-like 31-mult hash). */
function hashIndex(name: string, modulo: number): number {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) {
    hash = (hash * 31 + name.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) % modulo;
}

/** Accent colour CSS string for a connector tile, derived from its name. */
export function accentTokenFor(name: string): string {
  return ACCENT_TOKENS[hashIndex(name, ACCENT_TOKENS.length)];
}
