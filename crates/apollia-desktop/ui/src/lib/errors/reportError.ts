/**
 * Route a raw error to the right surface with humanized copy.
 *
 * Resolves the raw error through `humanize` (using the live `svelte-i18n`
 * formatter) and then:
 *  - `"toast"`  - fires a transient error toast (`ui/toast`).
 *  - `"inline"` - returns the `HumanizedError` for an inline `operator/ErrorBanner`.
 *  - `"banner"` - returns the `HumanizedError` for a persistent `ui/banner`.
 *
 * Every surface returns the resolved `HumanizedError`, so a caller can render
 * the banner/inline notice itself (and expose `detail` behind a Details
 * disclosure) regardless of the chosen surface.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { addToast } from "$lib/components/ui/toast";
import { humanize, type ErrorTranslate, type HumanizedError } from "./humanize";

export type ErrorSurface = "inline" | "toast" | "banner";

export interface ReportErrorOptions {
  /** Where to present the error. */
  surface: ErrorSurface;
  /** Optional `data-testid` forwarded to the toast (toast surface only). */
  testid?: string;
}

export function reportError(
  raw: unknown,
  options: ReportErrorOptions,
): HumanizedError {
  const translate = get(t) as ErrorTranslate;
  const humanized = humanize(raw, translate);

  if (options.surface === "toast") {
    addToast(humanized.friendly_message, "error", {
      "data-testid": options.testid,
    });
  }

  return humanized;
}
