import { register, init, getLocaleFromNavigator } from "svelte-i18n";

register("en", () => import("./en.json"));
register("fr", () => import("./fr.json"));

const LOCALE_STORAGE_KEY = "apollia-locale";

const savedLocale =
  typeof localStorage !== "undefined"
    ? localStorage.getItem(LOCALE_STORAGE_KEY)
    : null;

init({
  fallbackLocale: "en",
  initialLocale: savedLocale ?? getLocaleFromNavigator() ?? "en",
});

/** Persist the user's locale choice to localStorage. */
export function setLocale(locale: string): void {
  localStorage.setItem(LOCALE_STORAGE_KEY, locale);
}

export { LOCALE_STORAGE_KEY };
