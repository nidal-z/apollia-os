/**
 * The texts of a catalogue entry in the interface language.
 *
 * The selection sends each label, description and help text in every locale
 * it declares. These functions pick the one of the current language, then
 * English, then whichever is there, so an entry missing one translation still
 * shows a text. The public registry speaks a single language: its title and
 * description are shown as they come.
 *
 * Pure, so a component feeds `$locale` from `svelte-i18n` and a language
 * switch re-renders the text.
 */
import type { CatalogueText, ConnectorEnrichmentView, RegistryServerView } from "$lib/types";

/** The text of `text` for `locale` (`fr`, `fr-FR`), else English, else the first one given. */
export function pickLocale(
  text: CatalogueText | null | undefined,
  locale: string | null | undefined,
): string | null {
  if (!text) return null;
  const wanted = (locale ?? "").toLowerCase();
  const base = wanted.split("-")[0] ?? "";
  for (const key of [wanted, base, "en"]) {
    const value = key ? text[key] : undefined;
    if (typeof value === "string" && value.trim().length > 0) return value;
  }
  return Object.values(text).find((value) => typeof value === "string" && value.trim().length > 0) ?? null;
}

/** Every text of `text`, whatever its locale: what a search looks through. */
export function allLocales(text: CatalogueText | null | undefined): string[] {
  return text ? Object.values(text).filter((value) => typeof value === "string") : [];
}

/** The name an entry is shown under: its label in `locale`, else its title, else its identifier. */
export function entryLabel(
  entry: Pick<RegistryServerView, "name" | "title" | "enrichment">,
  locale: string | null | undefined,
): string {
  return pickLocale(entry.enrichment?.operator_label, locale) ?? entry.title ?? entry.name;
}

/** The description of an entry in `locale`, else the one the registry gave. */
export function entryDescription(
  entry: Pick<RegistryServerView, "description" | "enrichment">,
  locale: string | null | undefined,
): string | null {
  return pickLocale(entry.enrichment?.description, locale) ?? entry.description ?? null;
}

/** The label of an enrichment in `locale`, or `null` without one. */
export function enrichmentLabel(
  enrichment: ConnectorEnrichmentView | null | undefined,
  locale: string | null | undefined,
): string | null {
  return pickLocale(enrichment?.operator_label, locale);
}

/** The description of the package argument at `index`, in `locale`. */
export function argumentDescription(
  enrichment: ConnectorEnrichmentView | null | undefined,
  index: number,
  locale: string | null | undefined,
): string | null {
  return pickLocale(enrichment?.argument_descriptions?.[index], locale);
}
