// One-time migration of the former interface mode into the developer mode.
//
// Up to 0.2 the desktop kept an "operator" or "builder" choice in the
// browser storage. The developer mode replaces it and lives in `apollia.toml`.
// A user who had chosen Builder keeps everything Builder showed: the first
// launch turns the developer mode on, then the old key is forgotten.

/** The storage the old choice was kept in, narrowed to what is read here. */
export type LegacyModeStorage = Pick<Storage, "getItem" | "removeItem">;

const LEGACY_KEY = "apollia-ui-mode";

/**
 * What the old choice asks of the developer mode: `true` for Builder,
 * `false` for Operator, `null` when there is nothing to migrate.
 */
export function readLegacyChoice(storage: LegacyModeStorage | null): boolean | null {
  try {
    const stored = storage?.getItem(LEGACY_KEY) ?? null;
    return stored === null ? null : stored === "builder";
  } catch {
    return null;
  }
}

/** Forgets the old choice, so the migration runs once. */
export function forgetLegacyChoice(storage: LegacyModeStorage | null): void {
  try {
    storage?.removeItem(LEGACY_KEY);
  } catch {
    // Storage unavailable: nothing was read from it either.
  }
}
