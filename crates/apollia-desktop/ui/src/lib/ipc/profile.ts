/**
 * Typed Tauri IPC wrappers for the user profile domain.
 *
 * One thin function per `#[tauri::command]` backing the profile: the flat
 * key/value store persisted in `__user__`, its schema entries, the off-schema
 * memory facts (`extras`), and the destructive reset. Components call these
 * instead of `invoke()` directly, so command names and payload shapes live in
 * a single place.
 */
import { invoke } from "@tauri-apps/api/core";

/** A single profile key/value pair with provenance and schema membership. */
export interface ProfileEntryView {
  key: string;
  value: string;
  /** `onboarding` | `user` | `agent:<name>`. */
  written_by: string;
  created_at: string;
  updated_at: string;
  in_schema: boolean;
}

/** The whole profile as returned by `get_profile`. */
export interface UserProfileView {
  /** Entries declared in PROFILE_SCHEMA (the form fields). */
  schema_entries: ProfileEntryView[];
  /** Off-schema memory facts learned by agents over conversations. */
  extras: ProfileEntryView[];
  /** All entries (schema + extras) flattened. */
  entries: ProfileEntryView[];
  last_updated_at: string | null;
}

/** Fetch the whole user profile. Throws when the profile is not initialised. */
export async function getProfile(): Promise<UserProfileView> {
  return invoke<UserProfileView>("get_profile");
}

/** Create or update one profile entry. Server-side it is tagged `user`. */
export async function setProfileEntry(
  key: string,
  value: string,
): Promise<ProfileEntryView> {
  return invoke<ProfileEntryView>("set_profile_entry", {
    request: { key, value },
  });
}

/** Delete one profile entry by key. */
export async function deleteProfileEntry(key: string): Promise<void> {
  return invoke<void>("delete_profile_entry", { key });
}

/** Wipe every field and memory fact, then restart onboarding. */
export async function resetUserProfile(): Promise<void> {
  return invoke<void>("reset_user_profile");
}

/** Relaunch the onboarding agent (used after a reset). */
export async function triggerOnboarding(): Promise<void> {
  return invoke<void>("trigger_onboarding", { topic: null, profile: null });
}
