// Outbound links to the published documentation site.
//
// The site is bilingual and both locales serve the same routes: the English
// build sits at the root (`defaultLocale: 'en'` in `docs/site`) and the French
// one under a `/fr` prefix, page for page. An operator running the interface in
// French therefore has a French page for every link the application emits, and
// reaching it is a matter of one path segment.
//
// Before this module the six link sites each carried a hard-coded
// `https://docs.apollia.fr/...` string with no locale segment, so every
// outbound click landed on English regardless of the interface language.
// Building the URL here is what makes the locale impossible to forget: a
// literal anywhere else is refused by `docsUrlSites.test.ts`.

import { get } from "svelte/store";
import { locale as i18nLocale } from "svelte-i18n";

/** Root of the published documentation site, built from `docs/site` by CI. */
const DOCS_ORIGIN = "https://docs.apollia.fr";

/**
 * Path segment the documentation site serves a locale under. The site's
 * default locale is English and Docusaurus serves a default locale without a
 * prefix, so English maps to the empty string rather than to `/en`.
 */
const LOCALE_PREFIX: Record<string, string> = {
  en: "",
  fr: "/fr",
};

/**
 * Reduce an interface locale to the prefix the documentation site uses.
 *
 * Accepts a region-tagged tag (`fr-FR`) and an unknown or absent value, which
 * fall back to the default locale of the site, meaning no prefix at all.
 */
function localePrefix(activeLocale: string | null | undefined): string {
  if (!activeLocale) return "";
  const base = activeLocale.split("-")[0].toLowerCase();
  return LOCALE_PREFIX[base] ?? "";
}

/**
 * Build the documentation URL for `path` under an explicit locale.
 *
 * `path` is the route as the English site publishes it, leading slash included
 * (`/operator-help/agents/install-an-agent`); an empty path, or `/`, addresses
 * the home page of the locale. Pure, so a component can wrap it in `$derived`
 * on `$locale` and have its links follow a language change with no reload.
 */
export function docsUrlFor(activeLocale: string | null | undefined, path = ""): string {
  const prefix = localePrefix(activeLocale);
  const normalised = path === "/" ? "" : path;
  const suffix =
    normalised === "" || normalised.startsWith("/") ? normalised : `/${normalised}`;
  if (suffix === "") return `${DOCS_ORIGIN}${prefix}/`;
  return `${DOCS_ORIGIN}${prefix}${suffix}`;
}

/**
 * Build the documentation URL for `path` under the locale in force right now.
 *
 * For call sites that resolve a URL when the user acts (a click handler, an
 * error mapper) rather than when a component renders.
 */
export function docsUrl(path = ""): string {
  return docsUrlFor(get(i18nLocale), path);
}
