/**
 * The decisions of Settings > Profile, kept out of the Svelte components so
 * they run under Vitest (node environment) and read in one place.
 *
 * The form carries no copy of the profile schema: the fields, their labels
 * and their help come from Rust with every profile read (`get_profile_by_id`).
 */
import type {
  ProfileEntryView,
  ProfileFieldView,
  ProfileSummaryView,
} from "$lib/ipc/profile";

/** `true` when the interface locale is French. */
function isFrench(locale: string | null | undefined): boolean {
  return (locale ?? "").toLowerCase().startsWith("fr");
}

/** The label of a field in the interface locale. */
export function fieldLabel(field: ProfileFieldView, locale: string | null | undefined): string {
  return isFrench(locale) ? field.label_fr : field.label_en;
}

/** The help, or example, of a field in the interface locale. */
export function fieldHelp(field: ProfileFieldView, locale: string | null | undefined): string {
  return isFrench(locale) ? field.help_fr : field.help_en;
}

/** The values of the fields, by key, as the form edits them. */
export function fieldValues(fields: ProfileFieldView[]): Record<string, string> {
  return Object.fromEntries(fields.map((f) => [f.key, f.value]));
}

/** The fields whose value changed, trimmed; an emptied field maps to "". */
export function changedFields(
  fields: ProfileFieldView[],
  values: Record<string, string>,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const field of fields) {
    const next = (values[field.key] ?? "").trim();
    if (next !== field.value.trim()) out[field.key] = next;
  }
  return out;
}

/** What the menu of a profile can do. */
export type ProfileMenuAction = "rename" | "set_default" | "reset" | "delete";

/** One entry of the menu of a profile. */
export interface ProfileMenuEntry {
  action: ProfileMenuAction;
  /** Label key under `settings.profile.menu`. */
  labelKey: string;
  disabled: boolean;
  /** Why the entry is disabled, as an i18n key; `null` when it is not. */
  reasonKey: string | null;
  destructive: boolean;
}

/**
 * The menu of a profile. The default profile has no "set as default", and its
 * deletion is shown disabled with the reason, since the runtime refuses it.
 */
export function profileMenu(profile: Pick<ProfileSummaryView, "is_default">): ProfileMenuEntry[] {
  const entries: ProfileMenuEntry[] = [
    {
      action: "rename",
      labelKey: "settings.profile.menu.rename",
      disabled: false,
      reasonKey: null,
      destructive: false,
    },
  ];
  if (!profile.is_default) {
    entries.push({
      action: "set_default",
      labelKey: "settings.profile.menu.set_default",
      disabled: false,
      reasonKey: null,
      destructive: false,
    });
  }
  entries.push(
    {
      action: "reset",
      labelKey: "settings.profile.menu.reset",
      disabled: false,
      reasonKey: null,
      destructive: false,
    },
    {
      action: "delete",
      labelKey: "settings.profile.menu.delete",
      disabled: profile.is_default,
      reasonKey: profile.is_default ? "settings.profile.menu.delete_refused" : null,
      destructive: true,
    },
  );
  return entries;
}

/** How a reset can be confirmed. */
export type ResetChoice = "reset" | "reset_and_onboard";

/**
 * The confirmations of a reset: any profile can be reset alone; the default
 * one can also be followed by the onboarding conversation.
 */
export function resetChoices(profile: Pick<ProfileSummaryView, "is_default">): ResetChoice[] {
  return profile.is_default ? ["reset", "reset_and_onboard"] : ["reset"];
}

/** What a reset reaches. */
export interface ResetEffects {
  reset: (id: string) => Promise<unknown>;
  onboard: () => Promise<unknown>;
}

/**
 * Reset a profile, then relaunch the onboarding when the user chose it. The
 * onboarding never runs for a profile that is not the default, and never when
 * the reset itself failed.
 */
export async function applyReset(
  profile: Pick<ProfileSummaryView, "id" | "is_default">,
  choice: ResetChoice,
  effects: ResetEffects,
): Promise<void> {
  await effects.reset(profile.id);
  if (choice === "reset_and_onboard" && profile.is_default) {
    await effects.onboard();
  }
}

/** The profile to show once `deletedId` is gone: the default one. */
export function profileAfterDelete(
  profiles: ProfileSummaryView[],
  deletedId: string,
): string | null {
  const left = profiles.filter((p) => p.id !== deletedId);
  return (left.find((p) => p.is_default) ?? left[0])?.id ?? null;
}

/** The longest key a fact typed without developer mode receives. */
const FACT_KEY_MAX = 40;

/**
 * The key of a fact the user typed as a sentence. Without developer mode the
 * key is never shown, so it is derived from the value, made unique among the
 * keys the profile already holds.
 */
export function factKeyFor(value: string, existing: string[]): string {
  const slug = value
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, FACT_KEY_MAX)
    .replace(/-+$/g, "");
  const base = `fact.${slug || "note"}`;
  const taken = new Set(existing);
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base}-${n}`)) n += 1;
  return `${base}-${n}`;
}

/** The enrichment banner shows once name and role are set and a field is still empty. */
export function shouldOfferEnrichment(fields: ProfileFieldView[]): boolean {
  const value = (key: string) => fields.find((f) => f.key === key)?.value.trim() ?? "";
  if (!value("name") || !value("role")) return false;
  return fields.some((f) => !f.value.trim());
}

/** Map a raw `written_by` provenance to its i18n label key, or null. */
export function sourceLabelKey(src: string | undefined): string | null {
  if (!src) return null;
  if (src === "onboarding") return "settings.profile.source.onboarding";
  if (src === "user") return "settings.profile.source.user";
  if (src.startsWith("agent:")) return "settings.profile.source.agent";
  return null;
}

/** What the facts of a profile reach. */
export interface FactEffects {
  set: (profileId: string, key: string, value: string) => Promise<ProfileEntryView>;
}

/**
 * Add a fact to a profile. Without developer mode `key` is `null` and the
 * key is derived from the value. Returns the stored fact, or `null` when the
 * value (or a key typed in developer mode) is empty, so nothing is written.
 */
export async function addFact(
  profileId: string,
  value: string,
  key: string | null,
  existing: ProfileEntryView[],
  effects: FactEffects,
): Promise<ProfileEntryView | null> {
  const text = value.trim();
  if (!text) return null;
  const typed = key?.trim() ?? null;
  if (typed !== null && !typed) return null;
  const finalKey = typed ?? factKeyFor(text, existing.map((f) => f.key));
  return effects.set(profileId, finalKey, text);
}

/**
 * Save an edited fact. An empty or unchanged value writes nothing and
 * returns `null`, which the row reads as "leave edit mode".
 */
export async function editFact(
  profileId: string,
  fact: ProfileEntryView,
  next: string,
  effects: FactEffects,
): Promise<ProfileEntryView | null> {
  const text = next.trim();
  if (!text || text === fact.value) return null;
  return effects.set(profileId, fact.key, text);
}

/** Copy the value of a fact, never its key. */
export async function copyFactValue(
  fact: Pick<ProfileEntryView, "value">,
  clipboard: { writeText: (text: string) => Promise<void> },
): Promise<void> {
  await clipboard.writeText(fact.value);
}
