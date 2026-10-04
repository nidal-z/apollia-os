//! The profiles as the profile screen manages them: several named profiles,
//! one of them the default, each with the fields of the schema and its own
//! facts.
//!
//! The desktop running the runtime in its own process calls these functions
//! directly; a client of the daemon reaches the same functions through
//! `/api/v1/profiles`. A profile is named by its identifier, or by `default`
//! for whichever profile is the default.

use std::collections::BTreeMap;

use apollia_memory::profile_schema::{is_canonical, profile_fields, ProfileFieldType};
use apollia_memory::user_memory::{Profile, UserMemoryError, UserMemoryRepository, WrittenBy};
use serde::{Deserialize, Serialize};

use super::{view, with_repo, ProfileEntryView, ProfileError, ProfileStore};

/// The word that names the default profile in place of its identifier.
pub const DEFAULT_PROFILE_ALIAS: &str = "default";

/// One profile in the list of the profile screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProfileSummaryView {
    /// Stable identifier.
    pub id: String,
    /// Display name of the profile.
    pub name: String,
    /// `true` for the one default profile.
    pub is_default: bool,
    /// A line that tells the profile apart: its role, else its function.
    pub detail: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

/// One field of a profile, with what a form needs to draw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProfileFieldView {
    /// Storage key of the field.
    pub key: String,
    /// French label.
    pub label_fr: String,
    /// English label.
    pub label_en: String,
    /// French help or example.
    pub help_fr: String,
    /// English help or example.
    pub help_en: String,
    /// `true` when the field takes several lines.
    pub long_text: bool,
    /// Current value, empty when the field is not filled.
    pub value: String,
}

/// One profile in full: its summary, its fields in schema order, its facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProfileDetailView {
    /// The profile as the list shows it.
    pub profile: ProfileSummaryView,
    /// Every field of the schema, filled or not.
    pub fields: Vec<ProfileFieldView>,
    /// The facts retained about the user in this profile.
    pub facts: Vec<ProfileEntryView>,
}

/// A change to a profile: a new name, new field values, or both. An empty
/// value empties its field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProfileUpdate {
    /// New display name.
    #[serde(default)]
    pub name: Option<String>,
    /// New values, by field key.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

fn storage(error: UserMemoryError) -> ProfileError {
    match error {
        UserMemoryError::DefaultProfile(name) => ProfileError::IsDefault(name),
        UserMemoryError::InvalidKey(key) => ProfileError::InvalidKey(key),
        other => ProfileError::Storage(other),
    }
}

/// The profile `id` names, `default` naming the default profile.
fn resolve(repo: &UserMemoryRepository, id: &str) -> Result<Profile, ProfileError> {
    if id == DEFAULT_PROFILE_ALIAS {
        return repo.default_profile().map_err(storage);
    }
    repo.profile(id)
        .map_err(storage)?
        .ok_or_else(|| ProfileError::ProfileNotFound(id.to_owned()))
}

fn summary(profile: &Profile) -> ProfileSummaryView {
    let detail = profile
        .role
        .as_ref()
        .or(profile.function.as_ref())
        .filter(|d| !d.trim().is_empty())
        .cloned();
    ProfileSummaryView {
        id: profile.id.clone(),
        name: profile.name.clone(),
        is_default: profile.is_default,
        detail,
        created_at: profile.created_at.clone(),
        updated_at: profile.updated_at.clone(),
    }
}

fn detail(
    repo: &UserMemoryRepository,
    profile: &Profile,
) -> Result<ProfileDetailView, ProfileError> {
    let fields = profile_fields()
        .iter()
        .map(|f| ProfileFieldView {
            key: f.key.to_owned(),
            label_fr: f.label_fr.to_owned(),
            label_en: f.label_en.to_owned(),
            help_fr: f.help_fr.to_owned(),
            help_en: f.help_en.to_owned(),
            long_text: f.field_type == ProfileFieldType::LongText,
            value: profile.field(f.key).unwrap_or_default().to_owned(),
        })
        .collect();
    let facts = repo
        .list_all_in(profile)
        .map_err(storage)?
        .iter()
        .filter(|e| !e.in_schema)
        .map(view)
        .collect();
    Ok(ProfileDetailView {
        profile: summary(profile),
        fields,
        facts,
    })
}

/// Lists every profile, the default first.
///
/// # Errors
///
/// [`ProfileError::Storage`] when the repository cannot be read.
pub async fn list_profiles(
    store: ProfileStore<'_>,
) -> Result<Vec<ProfileSummaryView>, ProfileError> {
    with_repo(store, |repo| {
        Ok(repo
            .list_profiles()
            .map_err(storage)?
            .iter()
            .map(summary)
            .collect())
    })
    .await
}

/// Reads one profile in full.
///
/// # Errors
///
/// [`ProfileError::ProfileNotFound`] when no profile has `id`.
pub async fn read_profile_by_id(
    store: ProfileStore<'_>,
    id: String,
) -> Result<ProfileDetailView, ProfileError> {
    with_repo(store, move |repo| detail(repo, &resolve(repo, &id)?)).await
}

/// Creates a profile with empty fields, not the default.
///
/// # Errors
///
/// [`ProfileError::InvalidKey`] for an empty name.
pub async fn create_profile(
    store: ProfileStore<'_>,
    name: String,
) -> Result<ProfileSummaryView, ProfileError> {
    with_repo(store, move |repo| {
        Ok(summary(&repo.create_profile(&name).map_err(storage)?))
    })
    .await
}

/// Renames a profile and writes its fields, then returns it in full.
///
/// # Errors
///
/// [`ProfileError::InvalidKey`] for an empty name or a key outside the
/// schema, [`ProfileError::ProfileNotFound`] when no profile has `id`.
pub async fn update_profile(
    store: ProfileStore<'_>,
    id: String,
    update: ProfileUpdate,
) -> Result<ProfileDetailView, ProfileError> {
    with_repo(store, move |repo| {
        let profile = resolve(repo, &id)?;
        if let Some(key) = update.fields.keys().find(|k| !is_canonical(k)) {
            return Err(ProfileError::InvalidKey(key.clone()));
        }
        if let Some(name) = &update.name {
            repo.rename_profile(&profile.id, name).map_err(storage)?;
        }
        for (key, value) in &update.fields {
            repo.set_profile_field(&profile.id, key, Some(value.trim()))
                .map_err(storage)?;
        }
        detail(repo, &resolve(repo, &profile.id)?)
    })
    .await
}

/// Makes a profile the default; the previous default keeps its own facts.
///
/// # Errors
///
/// [`ProfileError::ProfileNotFound`] when no profile has `id`.
pub async fn set_default_profile(
    store: ProfileStore<'_>,
    id: String,
) -> Result<ProfileSummaryView, ProfileError> {
    with_repo(store, move |repo| {
        let profile = resolve(repo, &id)?;
        repo.set_default_profile(&profile.id).map_err(storage)?;
        Ok(summary(&resolve(repo, &profile.id)?))
    })
    .await
}

/// Deletes a profile and its facts. The default profile is refused.
///
/// # Errors
///
/// [`ProfileError::IsDefault`] for the default profile,
/// [`ProfileError::ProfileNotFound`] when no profile has `id`.
pub async fn delete_profile(store: ProfileStore<'_>, id: String) -> Result<(), ProfileError> {
    with_repo(store, move |repo| {
        let profile = resolve(repo, &id)?;
        repo.delete_profile(&profile.id).map_err(storage)
    })
    .await
}

/// Empties every field and the free text of a profile and deletes its facts;
/// returns how many entries were removed.
///
/// # Errors
///
/// [`ProfileError::ProfileNotFound`] when no profile has `id`.
pub async fn reset_profile_by_id(
    store: ProfileStore<'_>,
    id: String,
) -> Result<usize, ProfileError> {
    with_repo(store, move |repo| {
        let profile = resolve(repo, &id)?;
        repo.reset_in(&profile).map_err(storage)
    })
    .await
}

/// Retains a fact in a profile, written by the user, and returns it as
/// stored.
///
/// # Errors
///
/// [`ProfileError::EmptyKey`] for an empty key, [`ProfileError::InvalidKey`]
/// for a key of the schema or a reserved key.
pub async fn set_profile_fact(
    store: ProfileStore<'_>,
    id: String,
    key: String,
    value: String,
) -> Result<ProfileEntryView, ProfileError> {
    if key.trim().is_empty() {
        return Err(ProfileError::EmptyKey);
    }
    if is_canonical(&key) {
        return Err(ProfileError::InvalidKey(key));
    }
    with_repo(store, move |repo| {
        let profile = resolve(repo, &id)?;
        repo.set_in(&profile, &key, value.trim(), WrittenBy::User)
            .map_err(storage)?;
        repo.get_in(&profile, &key)
            .map_err(storage)?
            .map(|e| view(&e))
            .ok_or(ProfileError::MissingAfterWrite(key))
    })
    .await
}

/// Forgets a fact of a profile.
///
/// # Errors
///
/// [`ProfileError::NotFound`] when the profile holds no fact under `key`.
pub async fn delete_profile_fact(
    store: ProfileStore<'_>,
    id: String,
    key: String,
) -> Result<(), ProfileError> {
    if is_canonical(&key) {
        return Err(ProfileError::InvalidKey(key));
    }
    with_repo(store, move |repo| {
        let profile = resolve(repo, &id)?;
        match repo.forget_in(&profile, &key) {
            Ok(()) => Ok(()),
            Err(UserMemoryError::NotFound(_)) => Err(ProfileError::NotFound(key)),
            Err(other) => Err(storage(other)),
        }
    })
    .await
}

#[cfg(test)]
mod tests;
