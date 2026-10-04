/**
 * Typed Tauri IPC wrappers for the user profiles.
 *
 * One thin function per `#[tauri::command]` backing Settings > Profile:
 * several named profiles, one of them the default, each with the fields the
 * Rust schema declares and its own facts. A profile is named by its
 * identifier, or by `"default"` for whichever profile is the default.
 * Components call these instead of `invoke()` directly, so command names and
 * payload shapes live in a single place.
 */
import { invoke } from "@tauri-apps/api/core";

/** The word that names the default profile in place of its identifier. */
export const DEFAULT_PROFILE = "default";

/** One fact retained in a profile, with its provenance. */
export interface ProfileEntryView {
  key: string;
  value: string;
  /** `onboarding` | `user` | `agent:<name>`. */
  written_by: string;
  created_at: string;
  updated_at: string;
  in_schema: boolean;
}

/** One profile as the list shows it. */
export interface ProfileSummaryView {
  id: string;
  name: string;
  is_default: boolean;
  /** Its role, else its function; `null` when neither is filled. */
  detail: string | null;
  created_at: string;
  updated_at: string;
}

/** One field of a profile, as the Rust schema declares it. */
export interface ProfileFieldView {
  key: string;
  label_fr: string;
  label_en: string;
  help_fr: string;
  help_en: string;
  long_text: boolean;
  /** Empty when the field is not filled. */
  value: string;
}

/** One profile in full: summary, fields in schema order, facts. */
export interface ProfileDetailView {
  profile: ProfileSummaryView;
  fields: ProfileFieldView[];
  facts: ProfileEntryView[];
}

/** A change to a profile: a new name, new field values, or both. */
export interface ProfileUpdate {
  name?: string;
  fields?: Record<string, string>;
}

/** Every profile, the default first. */
export async function listProfiles(): Promise<ProfileSummaryView[]> {
  return invoke<ProfileSummaryView[]>("list_profiles");
}

/** Create a profile with empty fields, not the default. */
export async function createProfile(name: string): Promise<ProfileSummaryView> {
  return invoke<ProfileSummaryView>("create_profile", { name });
}

/** Read one profile in full. */
export async function getProfileById(id: string): Promise<ProfileDetailView> {
  return invoke<ProfileDetailView>("get_profile_by_id", { id });
}

/** Rename a profile and write its fields; an empty value empties a field. */
export async function updateProfile(
  id: string,
  update: ProfileUpdate,
): Promise<ProfileDetailView> {
  return invoke<ProfileDetailView>("update_profile", { id, update });
}

/** Delete a profile and its facts. The default profile is refused. */
export async function deleteProfile(id: string): Promise<void> {
  return invoke<void>("delete_profile", { id });
}

/** Make a profile the default. */
export async function setDefaultProfile(id: string): Promise<ProfileSummaryView> {
  return invoke<ProfileSummaryView>("set_default_profile", { id });
}

/** Empty a profile's fields and free text and delete its facts. */
export async function resetProfile(id: string): Promise<number> {
  return invoke<number>("reset_profile", { id });
}

/** Retain a fact in a profile. Server-side it is tagged `user`. */
export async function setProfileFact(
  id: string,
  key: string,
  value: string,
): Promise<ProfileEntryView> {
  return invoke<ProfileEntryView>("set_profile_fact", { id, key, value });
}

/** Forget a fact of a profile. */
export async function deleteProfileFact(id: string, key: string): Promise<void> {
  return invoke<void>("delete_profile_fact", { id, key });
}

/** Relaunch the onboarding agent (offered after resetting the default). */
export async function triggerOnboarding(): Promise<void> {
  return invoke<void>("trigger_onboarding", { topic: null, profile: null });
}
