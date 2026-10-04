//! UserMemoryRepository: the user profile, as every surface reads it.
//!
//! `user_memory.db` holds two things. The `profiles` table carries several
//! named profiles, one of them the default, each made of the fields of
//! [`crate::profile_schema::PROFILE_SCHEMA`]. The reserved `__user__`
//! semantic namespace keeps the free-form entries: facts written by agents and
//! any key outside the schema.
//!
//! The key-value API below reads and writes the **default** profile: a schema
//! key goes to its field, any other key to the namespace. Internal state
//! markers (onboarding bookkeeping) use a double-underscore prefix and are
//! hidden from the listing.
//!
//! The first open of a database written before profiles existed migrates the
//! former global profile into the default one ([`migration`]).

use std::path::Path;

use crate::profile_schema::{is_canonical, PROFILE_SCHEMA};
use crate::search::{MemorySearch, SearchQuery, SearchSource};
use crate::semantic::{RememberInput, SemanticMemory};
use crate::store::MemoryStore;

mod facts;
mod migration;
mod persona;
mod profiles;

pub use profiles::{Profile, DEFAULT_PROFILE_NAME};

/// Reserved SemanticMemory namespace for the free-form entries of the profile.
pub const USER_NAMESPACE: &str = "__user__";

/// All keys in `__user__` starting with this prefix are considered internal
/// state (onboarding bookkeeping) and are hidden from the profile listing
/// returned to the UI.
const INTERNAL_KEY_PREFIX: &str = "__";

/// Internal key: ISO 8601 timestamp of the last onboarding session.
const KEY_ONBOARDING_LAST_SESSION: &str = "__onboarding_last_session";

/// Internal key: `"true"` when the user dismissed onboarding.
const KEY_ONBOARDING_SKIPPED: &str = "__onboarding_skipped";

/// Internal key prefix: one entry per topic covered by the onboarding agent
/// (e.g. `__onboarding_topic_identity`).
const KEY_ONBOARDING_TOPIC_PREFIX: &str = "__onboarding_topic_";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Provenance of a profile entry: what wrote it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
pub enum WrittenBy {
    /// Written by the onboarding-agent during initial setup.
    Onboarding,
    /// Written explicitly by the user (Settings → Profile, CLI, IPC).
    User,
    /// Written by an agent during task execution (name = agent identifier).
    Agent(String),
}

impl WrittenBy {
    /// Serialization tag stored in the SemanticMemory `source` column.
    pub fn tag(&self) -> String {
        match self {
            Self::Onboarding => "onboarding".to_owned(),
            Self::User => "user".to_owned(),
            Self::Agent(name) => format!("agent:{name}"),
        }
    }

    /// Reconstructs a [`WrittenBy`] from a storage tag.
    pub fn from_tag(tag: &str) -> Self {
        if tag == "onboarding" {
            Self::Onboarding
        } else if tag == "user" {
            Self::User
        } else if let Some(name) = tag.strip_prefix("agent:") {
            Self::Agent(name.to_owned())
        } else {
            Self::Agent(tag.to_owned())
        }
    }
}

/// A single profile entry returned by recall operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProfileEntry {
    /// Flat storage key (e.g. `name`, `free_text`, or a free-form key).
    pub key: String,
    /// Plain-text value.
    pub value: String,
    /// Who wrote this entry. A profile field is the user's own and always
    /// reads [`WrittenBy::User`].
    pub written_by: WrittenBy,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
    /// `true` when [`Self::key`] matches a canonical [`PROFILE_SCHEMA`] entry.
    pub in_schema: bool,
}

/// Errors from [`UserMemoryRepository`] operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UserMemoryError {
    /// A storage operation failed.
    #[error("storage error: {0}")]
    StorageError(String),
    /// The requested entry was not found.
    #[error("entry not found: {0}")]
    NotFound(String),
    /// The provided key is empty or starts with the internal prefix.
    #[error("invalid key: {0}")]
    InvalidKey(String),
    /// The default profile cannot be deleted; another one has to be made the
    /// default first.
    #[error(
        "'{0}' is the default profile and cannot be deleted: make another profile the default first"
    )]
    DefaultProfile(String),
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

/// Repository for the user profiles and the free-form entries.
pub struct UserMemoryRepository {
    store: MemoryStore,
}

impl UserMemoryRepository {
    /// Opens (or creates) the user profile database at `db_path`, and
    /// migrates a former global profile into the default profile once.
    pub fn new(db_path: &Path) -> Result<Self, UserMemoryError> {
        let store =
            MemoryStore::open(db_path).map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        migration::run(&store)?;
        Ok(Self { store })
    }

    // -- Profiles --

    /// Returns the default profile.
    pub fn default_profile(&self) -> Result<Profile, UserMemoryError> {
        profiles::default_profile(self.store.conn())?
            .ok_or_else(|| UserMemoryError::NotFound("default profile".to_owned()))
    }

    /// Reads one profile by identifier, `None` when no profile has it.
    pub fn profile(&self, id: &str) -> Result<Option<Profile>, UserMemoryError> {
        profiles::get(self.store.conn(), id)
    }

    /// The profile `designated` names, or the default profile when nothing is
    /// designated or the designated identifier names no profile any more: a
    /// designation left behind by a deleted profile reads as no designation.
    pub fn profile_or_default(&self, designated: Option<&str>) -> Result<Profile, UserMemoryError> {
        if let Some(id) = designated.filter(|id| !id.trim().is_empty()) {
            if let Some(profile) = self.profile(id)? {
                return Ok(profile);
            }
            tracing::event!(
                tracing::Level::DEBUG,
                profile_id = %id,
                "user_memory.profile.designation_orphaned"
            );
        }
        self.default_profile()
    }

    /// Lists every profile, the default first.
    pub fn list_profiles(&self) -> Result<Vec<Profile>, UserMemoryError> {
        profiles::list(self.store.conn())
    }

    /// Creates a profile with empty fields, not the default, and returns it.
    pub fn create_profile(&self, name: &str) -> Result<Profile, UserMemoryError> {
        let name = Self::profile_name(name)?;
        profiles::insert(self.store.conn(), name, false)
    }

    /// Renames the profile `id`.
    pub fn rename_profile(&self, id: &str, name: &str) -> Result<(), UserMemoryError> {
        let name = Self::profile_name(name)?;
        profiles::rename(self.store.conn(), id, name)
    }

    /// Makes `id` the default profile; the previous default stops being one.
    pub fn set_default_profile(&self, id: &str) -> Result<(), UserMemoryError> {
        profiles::set_default(self.store.conn(), id)
    }

    /// Writes one field of the profile `id`; `None` or an empty value empties
    /// it. A key outside the schema is refused with
    /// [`UserMemoryError::InvalidKey`].
    pub fn set_profile_field(
        &self,
        id: &str,
        key: &str,
        value: Option<&str>,
    ) -> Result<(), UserMemoryError> {
        if !is_canonical(key) {
            return Err(UserMemoryError::InvalidKey(key.to_owned()));
        }
        let value = value.filter(|v| !v.is_empty());
        profiles::set_field(self.store.conn(), id, key, value)
    }

    /// Deletes the profile `id`. The default profile is refused with
    /// [`UserMemoryError::DefaultProfile`]; a designation that named the
    /// deleted profile then reads as the default ([`Self::profile_or_default`]).
    pub fn delete_profile(&self, id: &str) -> Result<(), UserMemoryError> {
        profiles::delete(self.store.conn(), id)
    }

    /// The filled fields of `profile` in schema order, as entries.
    fn profile_entries(profile: &Profile) -> Vec<ProfileEntry> {
        PROFILE_SCHEMA
            .iter()
            .filter_map(|f| Self::field_entry(profile, f.key))
            .collect()
    }

    fn profile_name(name: &str) -> Result<&str, UserMemoryError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(UserMemoryError::InvalidKey(
                "(empty profile name)".to_owned(),
            ));
        }
        Ok(name)
    }

    // -- Key-value view of the default profile --

    /// Returns `true` when the default profile is blank and no user-visible
    /// free-form entry is present.
    pub fn is_empty(&self) -> Result<bool, UserMemoryError> {
        if !self.default_profile()?.is_blank() {
            return Ok(false);
        }
        Ok(self.visible_extras()?.is_empty())
    }

    /// Upserts an entry: a schema key writes the field of the default profile,
    /// any other key a free-form entry with the given provenance.
    ///
    /// Empty keys and keys starting with the internal prefix `__` are
    /// rejected with [`UserMemoryError::InvalidKey`].
    pub fn set(
        &self,
        key: &str,
        value: &str,
        written_by: WrittenBy,
    ) -> Result<(), UserMemoryError> {
        let profile = self.default_profile()?;
        self.set_in(&profile, key, value, written_by)
    }

    /// Upserts an entry for the profile `profile`: a schema key writes its
    /// field, any other key a fact of that profile ([`facts`]).
    pub fn set_in(
        &self,
        profile: &Profile,
        key: &str,
        value: &str,
        written_by: WrittenBy,
    ) -> Result<(), UserMemoryError> {
        Self::validate_external_key(key)?;
        if is_canonical(key) {
            return profiles::set_field(self.store.conn(), &profile.id, key, Some(value));
        }
        self.write_raw_in(&facts::namespace_of(profile), key, value, written_by)
    }

    /// Reads a single entry by key.  Returns `Ok(None)` when missing.
    pub fn get(&self, key: &str) -> Result<Option<ProfileEntry>, UserMemoryError> {
        let profile = self.default_profile()?;
        self.get_in(&profile, key)
    }

    /// Reads a single entry of `profile` by key: a schema key reads its
    /// field, any other key a fact of that profile.
    pub fn get_in(
        &self,
        profile: &Profile,
        key: &str,
    ) -> Result<Option<ProfileEntry>, UserMemoryError> {
        if is_canonical(key) {
            return Ok(Self::field_entry(profile, key));
        }
        let sem = SemanticMemory::new(&self.store);
        let entry = sem
            .recall(&facts::namespace_of(profile), key)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(entry.map(|se| Self::semantic_to_profile(&se)))
    }

    /// Lists all user-visible entries: the filled fields of the default
    /// profile in schema order, then the free-form entries alphabetically.
    pub fn list_all(&self) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        let profile = self.default_profile()?;
        self.list_all_in(&profile)
    }

    /// Lists the filled fields of `profile` in schema order, then its facts
    /// alphabetically.
    pub fn list_all_in(&self, profile: &Profile) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        let mut ordered = Self::profile_entries(profile);
        let mut extras = self.visible_extras_in(&facts::namespace_of(profile))?;
        extras.sort_by(|a, b| a.key.cmp(&b.key));
        ordered.extend(extras);
        Ok(ordered)
    }

    /// Lists only the filled fields of the default profile.
    pub fn list_schema(&self) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        Ok(self
            .list_all()?
            .into_iter()
            .filter(|e| e.in_schema)
            .collect())
    }

    /// Lists only free-form entries (not in schema).
    pub fn list_extras(&self) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        Ok(self
            .list_all()?
            .into_iter()
            .filter(|e| !e.in_schema)
            .collect())
    }

    /// Deletes a single entry by key: a schema key empties the field of the
    /// default profile.  Returns [`UserMemoryError::NotFound`] when the key
    /// held nothing.
    pub fn forget(&self, key: &str) -> Result<(), UserMemoryError> {
        let profile = self.default_profile()?;
        self.forget_in(&profile, key)
    }

    /// Empties the default profile and deletes its facts.  Internal state
    /// markers (onboarding bookkeeping) are preserved.  Returns the number of
    /// removed entries.
    pub fn reset(&self) -> Result<usize, UserMemoryError> {
        let profile = self.default_profile()?;
        self.reset_in(&profile)
    }

    /// Updates the value of an existing entry, preserving its provenance.
    /// Returns [`UserMemoryError::NotFound`] when the key does not exist.
    pub fn update(&self, key: &str, value: &str) -> Result<(), UserMemoryError> {
        Self::validate_external_key(key)?;
        let existing = self.get(key)?;
        let Some(entry) = existing else {
            return Err(UserMemoryError::NotFound(key.to_owned()));
        };
        self.set(key, value, entry.written_by)
    }

    /// Searches the user-visible entries: the fields of the default profile
    /// whose value contains `query` (case-insensitive), then the full-text
    /// matches among the free-form entries.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        let needle = query.to_lowercase();
        let profile = self.default_profile()?;
        let mut entries: Vec<ProfileEntry> = PROFILE_SCHEMA
            .iter()
            .filter_map(|f| Self::field_entry(&profile, f.key))
            .filter(|e| !needle.is_empty() && e.value.to_lowercase().contains(&needle))
            .take(limit)
            .collect();

        let searcher = MemorySearch::new(&self.store);
        let sem = SemanticMemory::new(&self.store);
        let results = searcher
            .query(SearchQuery {
                namespace: USER_NAMESPACE,
                query,
                limit: limit as u32,
                sources: Some(&[SearchSource::Semantic]),
                min_importance: None,
                meaning: None,
            })
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        for result in &results {
            if entries.len() >= limit {
                break;
            }
            if let Some(se) = all.iter().find(|e| e.id == result.source_id) {
                if se.key.starts_with(INTERNAL_KEY_PREFIX) || is_canonical(&se.key) {
                    continue;
                }
                entries.push(Self::semantic_to_profile(se));
            }
        }
        Ok(entries)
    }

    // -- Generic internal state (hidden from profile UI) --

    /// Reads an internal state value.  The key is automatically prefixed
    /// with `__` in storage, ensuring it is hidden from the profile listing.
    pub fn get_internal(&self, key: &str) -> Result<Option<String>, UserMemoryError> {
        let storage_key = format!("{INTERNAL_KEY_PREFIX}{key}");
        let entry = SemanticMemory::new(&self.store)
            .recall(USER_NAMESPACE, &storage_key)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(entry.and_then(|e| e.value.as_str().map(|s| s.to_owned())))
    }

    /// Writes an internal state value.  The key is automatically prefixed
    /// with `__` in storage, ensuring it is hidden from the profile listing.
    pub fn set_internal(&self, key: &str, value: &str) -> Result<(), UserMemoryError> {
        let storage_key = format!("{INTERNAL_KEY_PREFIX}{key}");
        self.write_raw(&storage_key, value, WrittenBy::User)
    }

    /// Removes an internal state value.  Symmetric to [`Self::set_internal`].
    /// Returns `Ok(())` whether the key existed or not; internal-state wipes
    /// should be tolerant of missing entries.
    pub fn forget_internal(&self, key: &str) -> Result<(), UserMemoryError> {
        let storage_key = format!("{INTERNAL_KEY_PREFIX}{key}");
        let sem = SemanticMemory::new(&self.store);
        sem.forget(USER_NAMESPACE, &storage_key)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(())
    }

    // -- Onboarding bookkeeping (internal state, hidden from profile UI) --

    /// Returns the list of onboarding topics already covered.
    pub fn get_covered_topics(&self) -> Result<Vec<String>, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(all
            .into_iter()
            .filter_map(|e| {
                e.key
                    .strip_prefix(KEY_ONBOARDING_TOPIC_PREFIX)
                    .map(|t| t.to_owned())
            })
            .collect())
    }

    /// Marks an onboarding topic as covered.
    pub fn mark_topic_covered(&self, topic: &str) -> Result<(), UserMemoryError> {
        let key = format!("{KEY_ONBOARDING_TOPIC_PREFIX}{topic}");
        self.write_raw(&key, "covered", WrittenBy::Onboarding)
    }

    /// Returns `true` when the user dismissed the onboarding.
    pub fn get_onboarding_skipped(&self) -> Result<bool, UserMemoryError> {
        let entry = SemanticMemory::new(&self.store)
            .recall(USER_NAMESPACE, KEY_ONBOARDING_SKIPPED)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(entry
            .and_then(|e| e.value.as_str().map(|s| s.to_owned()))
            .map(|v| v == "true")
            .unwrap_or(false))
    }

    /// Marks or unmarks the onboarding as skipped.
    pub fn set_onboarding_skipped(&self, skipped: bool) -> Result<(), UserMemoryError> {
        self.write_raw(
            KEY_ONBOARDING_SKIPPED,
            if skipped { "true" } else { "false" },
            WrittenBy::Onboarding,
        )
    }

    /// Returns the ISO 8601 timestamp of the last onboarding session, if any.
    pub fn get_last_onboarding_session(&self) -> Result<Option<String>, UserMemoryError> {
        let entry = SemanticMemory::new(&self.store)
            .recall(USER_NAMESPACE, KEY_ONBOARDING_LAST_SESSION)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(entry.and_then(|e| e.value.as_str().map(|s| s.to_owned())))
    }

    /// Records the timestamp of the current onboarding session.
    pub fn set_last_onboarding_session(&self, timestamp: &str) -> Result<(), UserMemoryError> {
        self.write_raw(
            KEY_ONBOARDING_LAST_SESSION,
            timestamp,
            WrittenBy::Onboarding,
        )
    }

    // -- Internal helpers --

    /// Returns `true` if `iso_ts` is older than 180 days from now.
    fn is_stale(iso_ts: &str) -> bool {
        let Some(date_str) = iso_ts.get(..10) else {
            return false;
        };
        let Ok(then) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            return false;
        };
        let now = chrono::Utc::now().date_naive();
        now.signed_duration_since(then).num_days() > 180
    }

    /// Validates an externally-supplied key.  Empty keys and `__`-prefixed
    /// keys are rejected.
    fn validate_external_key(key: &str) -> Result<(), UserMemoryError> {
        if key.is_empty() {
            return Err(UserMemoryError::InvalidKey("(empty)".to_owned()));
        }
        if key.starts_with(INTERNAL_KEY_PREFIX) {
            return Err(UserMemoryError::InvalidKey(key.to_owned()));
        }
        Ok(())
    }

    /// Writes an entry without external-key validation, used by both the
    /// canonical API (after validation) and internal-state setters.
    fn write_raw(
        &self,
        key: &str,
        value: &str,
        written_by: WrittenBy,
    ) -> Result<(), UserMemoryError> {
        self.write_raw_in(USER_NAMESPACE, key, value, written_by)
    }

    /// [`Self::write_raw`] into `namespace`.
    fn write_raw_in(
        &self,
        namespace: &str,
        key: &str,
        value: &str,
        written_by: WrittenBy,
    ) -> Result<(), UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let json_value = serde_json::Value::String(value.to_owned());
        let source_tag = written_by.tag();
        sem.remember(RememberInput {
            namespace,
            key,
            value: &json_value,
            confidence: 1.0,
            source: Some(&source_tag),
            expires_at: None,
        })
        .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// The entry of one field of `profile`, `None` when the field is empty.
    fn field_entry(profile: &Profile, key: &str) -> Option<ProfileEntry> {
        let field = PROFILE_SCHEMA.iter().find(|f| f.key == key)?;
        profile.field(field.key).map(|value| ProfileEntry {
            key: field.key.to_owned(),
            value: value.to_owned(),
            written_by: WrittenBy::User,
            created_at: profile.created_at.clone(),
            updated_at: profile.updated_at.clone(),
            in_schema: true,
        })
    }

    /// The user-visible free-form entries of the namespace, unordered. A key
    /// that names a schema field is not one: the field lives in the profile.
    fn visible_extras(&self) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        self.visible_extras_in(USER_NAMESPACE)
    }

    /// [`Self::visible_extras`] of `namespace`.
    fn visible_extras_in(&self, namespace: &str) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let all = sem
            .recall_all(namespace, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(all
            .iter()
            .filter(|e| !e.key.starts_with(INTERNAL_KEY_PREFIX) && !is_canonical(&e.key))
            .map(Self::semantic_to_profile)
            .collect())
    }

    /// Converts a [`crate::semantic::SemanticEntry`] to a [`ProfileEntry`].
    fn semantic_to_profile(se: &crate::semantic::SemanticEntry) -> ProfileEntry {
        let value = se
            .value
            .as_str()
            .map(|s| s.to_owned())
            .unwrap_or_else(|| se.value.to_string());
        let written_by = se
            .source
            .as_deref()
            .map(WrittenBy::from_tag)
            .unwrap_or_else(|| WrittenBy::Agent("unknown".to_owned()));
        ProfileEntry {
            key: se.key.clone(),
            value,
            written_by,
            created_at: se.created_at.clone(),
            updated_at: se.updated_at.clone(),
            in_schema: false,
        }
    }
}

#[cfg(test)]
mod tests;
