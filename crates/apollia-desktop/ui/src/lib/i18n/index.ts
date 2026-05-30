import { register, init, getLocaleFromNavigator } from "svelte-i18n";

register("en", () => import("./en.json"));
register("fr", () => import("./fr.json"));

const LOCALE_STORAGE_KEY = "apollia-locale";
const SUPPORTED_LOCALES = ["fr", "en"] as const;
const DEFAULT_LOCALE = "fr";

const savedLocale =
  typeof localStorage === "undefined"
    ? null
    : localStorage.getItem(LOCALE_STORAGE_KEY);

function resolveInitialLocale(): string {
  if (savedLocale && SUPPORTED_LOCALES.includes(savedLocale as (typeof SUPPORTED_LOCALES)[number])) {
    return savedLocale;
  }
  const navigator = getLocaleFromNavigator();
  if (navigator) {
    const base = navigator.split("-")[0].toLowerCase();
    if (SUPPORTED_LOCALES.includes(base as (typeof SUPPORTED_LOCALES)[number])) {
      return base;
    }
  }
  return DEFAULT_LOCALE;
}

init({
  fallbackLocale: "en",
  initialLocale: resolveInitialLocale(),
});

/** Persist the user's locale choice to localStorage. */
export function setLocale(locale: string): void {
  localStorage.setItem(LOCALE_STORAGE_KEY, locale);
}

export { LOCALE_STORAGE_KEY, SUPPORTED_LOCALES, DEFAULT_LOCALE };
