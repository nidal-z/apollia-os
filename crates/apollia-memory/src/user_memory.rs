//! UserMemoryRepository — canonical user profile backed by SemanticMemory.
//!
//! Persists the global user profile under the reserved `__user__` namespace.
//! Keys are flat (no `category.` or `user.` prefix) and either match a
//! canonical [`crate::profile_schema::PROFILE_SCHEMA`] entry or live as
//! "extras" (free-form, surfaced separately in the UI).
//!
//! Internal state markers (onboarding bookkeeping, migration receipts) use a
//! double-underscore prefix and are hidden from the profile listing.
//!
//! ADR-087 amends ADR-038: the namespace and the SemanticMemory backend are
//! preserved, but the public API and storage layout are simplified.

use std::fmt;
use std::path::Path;

use crate::profile_schema::{field_for, is_canonical, PROFILE_SCHEMA};
use crate::search::{MemorySearch, SearchSource};
use crate::semantic::SemanticMemory;
use crate::store::MemoryStore;

/// Reserved SemanticMemory namespace for the global user profile.
pub const USER_NAMESPACE: &str = "__user__";

/// All keys in `__user__` starting with this prefix are considered internal
/// state (onboarding bookkeeping, migration receipts) and are hidden from the
/// profile listing returned to the UI.
const INTERNAL_KEY_PREFIX: &str = "__";

/// Internal key: ISO 8601 timestamp of the last onboarding session.
const KEY_ONBOARDING_LAST_SESSION: &str = "__onboarding_last_session";

/// Internal key: `"true"` when the user dismissed onboarding.
const KEY_ONBOARDING_SKIPPED: &str = "__onboarding_skipped";

/// Internal key prefix: one entry per topic covered by the onboarding agent
/// (e.g. `__onboarding_topic_identity`).
const KEY_ONBOARDING_TOPIC_PREFIX: &str = "__onboarding_topic_";

// ---------------------------------------------------------------------------
// Public types — canonical (ADR-087)
// ---------------------------------------------------------------------------

/// Provenance of a profile entry — what wrote it.
///
/// Replaces the legacy 4-variant [`UserMemorySource`] with 3 cases; agent
/// observations (including legacy `chat_inference`) collapse into
/// [`WrittenBy::Agent`] tagged with the agent name.
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

    /// Reconstructs a [`WrittenBy`] from a storage tag.  Unknown or legacy
    /// tags are best-effort mapped: `user_explicit` → [`Self::User`],
    /// `chat_inference`/`agent_observation` → [`Self::Agent`] with a
    /// descriptive name.
    pub fn from_tag(tag: &str) -> Self {
        if tag == "onboarding" {
            Self::Onboarding
        } else if tag == "user" || tag == "user_explicit" {
            Self::User
        } else if let Some(name) = tag.strip_prefix("agent:") {
            Self::Agent(name.to_owned())
        } else if tag == "chat_inference" {
            Self::Agent("chat-extractor".to_owned())
        } else if tag == "agent_observation" {
            Self::Agent("legacy".to_owned())
        } else {
            Self::Agent("legacy".to_owned())
        }
    }
}

/// A single profile entry returned by recall operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProfileEntry {
    /// Flat storage key (e.g. `name`, `agents.hitl`).
    pub key: String,
    /// Plain-text value.
    pub value: String,
    /// Who wrote this entry.
    pub written_by: WrittenBy,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
    /// `true` when [`key`] matches a canonical [`PROFILE_SCHEMA`] entry.
    pub in_schema: bool,
}

/// Errors from [`UserMemoryRepository`] operations.
#[derive(Debug, thiserror::Error)]
pub enum UserMemoryError {
    /// A storage operation failed.
    #[error("storage error: {0}")]
    StorageError(String),
    /// The provided category string is not recognized (legacy API).
    #[error("invalid category: {0}")]
    InvalidCategory(String),
    /// The requested entry was not found.
    #[error("entry not found: {0}")]
    NotFound(String),
    /// The provided key is empty or starts with the internal prefix.
    #[error("invalid key: {0}")]
    InvalidKey(String),
}

// ---------------------------------------------------------------------------
// Legacy types — kept for backward compatibility (deprecated)
// ---------------------------------------------------------------------------

/// Category of a user memory entry.
///
/// **Deprecated (ADR-087)**: kept for backward compatibility while callers
/// migrate.  Storage no longer uses categories; calls receiving a category
/// argument simply ignore it on the write side, and on the read side return
/// schema entries grouped by the corresponding [`crate::profile_schema::ProfileSection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryCategory {
    /// User profile identity (legacy).
    Profile,
    /// User preferences (legacy).
    Preferences,
    /// Observed user habits (legacy).
    Habits,
    /// Contextual information (legacy).
    Context,
}

impl UserMemoryCategory {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Preferences => "preferences",
            Self::Habits => "habits",
            Self::Context => "context",
        }
    }
}

impl fmt::Display for UserMemoryCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Source of a user memory entry.
///
/// **Deprecated (ADR-087)**: use [`WrittenBy`] instead.  Conversion is provided
/// via [`Self::into_written_by`] and [`From`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemorySource {
    /// Set during onboarding (legacy).
    Onboarding,
    /// Inferred by the LLM from a chat conversation (legacy).
    ChatInference,
    /// Explicitly provided by the user (legacy).
    UserExplicit,
    /// Observed by an agent during task execution (legacy).
    AgentObservation,
}

impl UserMemorySource {
    /// Converts a legacy source enum into the canonical [`WrittenBy`].
    pub fn into_written_by(self) -> WrittenBy {
        match self {
            Self::Onboarding => WrittenBy::Onboarding,
            Self::UserExplicit => WrittenBy::User,
            Self::ChatInference => WrittenBy::Agent("chat-extractor".to_owned()),
            Self::AgentObservation => WrittenBy::Agent("legacy".to_owned()),
        }
    }
}

impl From<UserMemorySource> for WrittenBy {
    fn from(src: UserMemorySource) -> Self {
        src.into_written_by()
    }
}

impl fmt::Display for UserMemorySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Onboarding => "onboarding",
            Self::ChatInference => "chat_inference",
            Self::UserExplicit => "user_explicit",
            Self::AgentObservation => "agent_observation",
        };
        f.write_str(s)
    }
}

/// A single user memory entry — legacy shape.
///
/// **Deprecated (ADR-087)**: use [`ProfileEntry`] instead.  The `category`
/// field is derived best-effort from the canonical schema section.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserMemoryEntry {
    /// Legacy category (derived).
    pub category: UserMemoryCategory,
    /// Short key.
    pub key: String,
    /// Plain-text value.
    pub value: String,
    /// Legacy source.
    pub source: UserMemorySource,
    /// Confidence score — always `1.0` in the simplified model.
    pub confidence: f64,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

/// Repository for the global user profile, backed by [`SemanticMemory`].
///
/// All entries live under the [`USER_NAMESPACE`] namespace, with flat keys.
/// Canonical fields are described by [`PROFILE_SCHEMA`]; free-form keys are
/// tolerated and surfaced as "extras".
pub struct UserMemoryRepository {
    store: MemoryStore,
}

impl UserMemoryRepository {
    /// Opens (or creates) the user memory database at `db_path`.
    pub fn new(db_path: &Path) -> Result<Self, UserMemoryError> {
        let store =
            MemoryStore::open(db_path).map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(Self { store })
    }

    /// Returns `true` when no user-visible (non-internal) entry is present.
    pub fn is_empty(&self) -> Result<bool, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(all
            .iter()
            .all(|e| e.key.starts_with(INTERNAL_KEY_PREFIX)))
    }

    // -- Canonical API (ADR-087) --

    /// Upserts a profile entry under [`key`] with the given provenance.
    ///
    /// Empty keys and keys starting with the internal prefix `__` are
    /// rejected with [`UserMemoryError::InvalidKey`].
    pub fn set(
        &self,
        key: &str,
        value: &str,
        written_by: WrittenBy,
    ) -> Result<(), UserMemoryError> {
        Self::validate_external_key(key)?;
        self.write_raw(key, value, written_by)
    }

    /// Reads a single profile entry by key.  Returns `Ok(None)` when missing.
    pub fn get(&self, key: &str) -> Result<Option<ProfileEntry>, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let entry = sem
            .recall(USER_NAMESPACE, key)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(entry.map(|se| Self::semantic_to_profile(&se)))
    }

    /// Lists all user-visible entries (schema fields + extras), ordered by
    /// schema position first, then alphabetically for extras.
    pub fn list_all(&self) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        let mut by_key: std::collections::HashMap<String, ProfileEntry> = all
            .into_iter()
            .filter(|e| !e.key.starts_with(INTERNAL_KEY_PREFIX))
            .map(|e| (e.key.clone(), Self::semantic_to_profile(&e)))
            .collect();

        let mut ordered = Vec::with_capacity(by_key.len());
        for field in PROFILE_SCHEMA {
            if let Some(entry) = by_key.remove(field.key) {
                ordered.push(entry);
            }
        }
        let mut extras: Vec<ProfileEntry> = by_key.into_values().collect();
        extras.sort_by(|a, b| a.key.cmp(&b.key));
        ordered.extend(extras);
        Ok(ordered)
    }

    /// Lists only canonical schema entries (filtered subset of [`list_all`]).
    pub fn list_schema(&self) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        Ok(self.list_all()?.into_iter().filter(|e| e.in_schema).collect())
    }

    /// Lists only free-form entries (not in schema).
    pub fn list_extras(&self) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        Ok(self
            .list_all()?
            .into_iter()
            .filter(|e| !e.in_schema)
            .collect())
    }

    /// Deletes a single entry by key.  Returns [`UserMemoryError::NotFound`]
    /// when the key did not exist.
    pub fn forget(&self, key: &str) -> Result<(), UserMemoryError> {
        Self::validate_external_key(key)?;
        let sem = SemanticMemory::new(&self.store);
        let deleted = sem
            .forget(USER_NAMESPACE, key)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        if deleted {
            Ok(())
        } else {
            Err(UserMemoryError::NotFound(key.to_owned()))
        }
    }

    /// Deletes every user-visible entry.  Internal state markers (onboarding
    /// bookkeeping, migration receipts) are preserved.  Returns the number of
    /// deleted entries.
    pub fn reset(&self) -> Result<usize, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        let mut count = 0usize;
        for entry in all {
            if entry.key.starts_with(INTERNAL_KEY_PREFIX) {
                continue;
            }
            let removed = sem
                .forget(USER_NAMESPACE, &entry.key)
                .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
            if removed {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Updates the value of an existing entry, preserving its provenance.
    /// Returns [`UserMemoryError::NotFound`] when the key does not exist.
    pub fn update(&self, key: &str, value: &str) -> Result<(), UserMemoryError> {
        Self::validate_external_key(key)?;
        let existing = self.get(key)?;
        let Some(entry) = existing else {
            return Err(UserMemoryError::NotFound(key.to_owned()));
        };
        self.write_raw(key, value, entry.written_by)
    }

    /// Full-text search across user-visible entries.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ProfileEntry>, UserMemoryError> {
        let searcher = MemorySearch::new(&self.store);
        let sem = SemanticMemory::new(&self.store);

        let results = searcher
            .query(
                USER_NAMESPACE,
                query,
                limit as u32,
                Some(&[SearchSource::Semantic]),
                None,
            )
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        let mut entries = Vec::with_capacity(results.len());
        for result in &results {
            if let Some(se) = all.iter().find(|e| e.id == result.source_id) {
                if se.key.starts_with(INTERNAL_KEY_PREFIX) {
                    continue;
                }
                entries.push(Self::semantic_to_profile(se));
            }
        }
        Ok(entries)
    }

    /// Produces a text block suitable for LLM system-prompt injection
    /// (legacy `recall_all_for_injection` shape, kept for chat user_context).
    ///
    /// Output groups entries by schema section:
    /// ```text
    /// Section: identity
    /// - name: Nidal
    /// - role: CTO
    /// Section: preferences
    /// - preferences.language: fr
    /// ```
    pub fn recall_all_for_injection(&self, max_entries: usize) -> Result<String, UserMemoryError> {
        let entries = self.list_all()?;
        let mut output = String::new();
        let mut total = 0usize;

        let sections = [
            crate::profile_schema::ProfileSection::Identity,
            crate::profile_schema::ProfileSection::Work,
            crate::profile_schema::ProfileSection::Preferences,
            crate::profile_schema::ProfileSection::Constraints,
        ];

        for section in sections {
            let in_section: Vec<&ProfileEntry> = entries
                .iter()
                .filter(|e| field_for(&e.key).map(|f| f.section) == Some(section))
                .collect();
            if in_section.is_empty() {
                continue;
            }
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!("Section: {}\n", section.tag()));
            for entry in in_section {
                if total >= max_entries {
                    break;
                }
                output.push_str(&format!("- {}: {}\n", entry.key, entry.value));
                total += 1;
            }
            if total >= max_entries {
                break;
            }
        }

        // Append extras under a trailing section.
        let extras: Vec<&ProfileEntry> = entries.iter().filter(|e| !e.in_schema).collect();
        if !extras.is_empty() && total < max_entries {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str("Section: other\n");
            for entry in extras {
                if total >= max_entries {
                    break;
                }
                output.push_str(&format!("- {}: {}\n", entry.key, entry.value));
                total += 1;
            }
        }

        Ok(output)
    }

    /// Produces a structured persona brief for LLM system-prompt injection.
    ///
    /// Unlike [`Self::recall_all_for_injection`], this method renders a French
    /// narrative summary aimed at agent system prompts.  Reads flat canonical
    /// keys (no `category.` prefix); extras under `goal*` / `tools*` are
    /// folded into the brief.
    pub fn recall_persona_brief(&self, max_entries: usize) -> Result<String, UserMemoryError> {
        let entries = self.list_all()?;
        if entries.is_empty() {
            return Ok(String::new());
        }

        let find = |k: &str| -> Option<String> {
            entries
                .iter()
                .find(|e| e.key == k)
                .map(|e| e.value.clone())
        };

        let name = find("name");
        let role = find("role");
        let industry = find("domain.sector");
        let team_size = find("domain.team_size");
        let expertise = find("tech.proficiency");
        let language = find("preferences.language");
        let hitl = find("agents.hitl");
        let sovereignty = find("constraints.sovereignty");
        let compliance = find("constraints.compliance");

        let mut output = String::new();

        // Narrative header
        let mut headline_parts: Vec<String> = Vec::new();
        if let Some(ref n) = name {
            headline_parts.push(n.clone());
        }
        if let Some(ref r) = role {
            if headline_parts.is_empty() {
                headline_parts.push(format!("Rôle : {r}"));
            } else {
                headline_parts.push(format!("({r})"));
            }
        }
        if let Some(ref ind) = industry {
            headline_parts.push(format!("— {ind}"));
        }
        if !headline_parts.is_empty() {
            output.push_str(&headline_parts.join(" "));
            if let Some(ref exp) = expertise {
                output.push_str(&format!(". Niveau : {exp}"));
            }
            if let Some(ref ts) = team_size {
                output.push_str(&format!(". Équipe : {ts}"));
            }
            output.push('\n');
        }

        // Governance
        let mut gov_parts: Vec<String> = Vec::new();
        if let Some(ref h) = hitl {
            let label = match h.as_str() {
                "always" => "supervision systématique",
                "critical-only" => "supervision sur actions critiques",
                "never" => "autonomie complète demandée",
                other => other,
            };
            gov_parts.push(format!("Supervision : {label}"));
        }
        if let Some(ref s) = sovereignty {
            let label = match s.as_str() {
                "local-only" => "données strictement locales",
                "local-preferred" => "local préféré, cloud en dernier recours",
                "cloud-ok" => "cloud autorisé",
                other => other,
            };
            gov_parts.push(format!("Souveraineté : {label}"));
        }
        if let Some(ref c) = compliance {
            if !c.trim().is_empty() {
                gov_parts.push(format!("Conformité : {c}"));
            }
        }
        if !gov_parts.is_empty() {
            output.push_str(&gov_parts.join(" | "));
            output.push('\n');
        }

        // Preferences
        if let Some(ref lang) = language {
            output.push_str(&format!("Langue : {lang}\n"));
        }

        // Goals
        let goals: Vec<String> = entries
            .iter()
            .filter(|e| e.key == "goals" || e.key.starts_with("goal"))
            .map(|e| e.value.clone())
            .collect();
        if !goals.is_empty() {
            output.push_str("\nObjectifs :\n");
            for g in &goals {
                output.push_str(&format!("- {g}\n"));
            }
        }

        // Daily tools
        let tools: Vec<String> = entries
            .iter()
            .filter(|e| e.key == "tools.daily" || e.key.starts_with("tools"))
            .map(|e| e.value.clone())
            .collect();
        if !tools.is_empty() {
            output.push_str("\nOutils : ");
            output.push_str(&tools.join(", "));
            output.push('\n');
        }

        // Adaptation hints
        output.push_str("\nAdaptation :\n");
        if let Some(ref lang) = language {
            output.push_str(&format!("- Langue : {lang}\n"));
        }
        if let Some(ref r) = role {
            let r_lower = r.to_lowercase();
            if r_lower.contains("dev")
                || r_lower.contains("engineer")
                || r_lower.contains("data")
                || r_lower.contains("devops")
                || r_lower.contains("sysadmin")
            {
                output.push_str("- Profil technique : vocabulaire technique approprié\n");
            } else if r_lower.contains("design")
                || r_lower.contains("creat")
                || r_lower.contains("vidéo")
                || r_lower.contains("video")
            {
                output.push_str(
                    "- Profil créatif : éviter le jargon technique, privilégier les visuels\n",
                );
            } else if r_lower.contains("manager")
                || r_lower.contains("market")
                || r_lower.contains("commercial")
                || r_lower.contains("sales")
                || r_lower.contains("rh")
                || r_lower.contains("ceo")
                || r_lower.contains("cto")
                || r_lower.contains("coo")
            {
                output.push_str(
                    "- Profil business : focus résultats, KPIs, pas de détails techniques inutiles\n",
                );
            } else {
                output.push_str("- Adapter le vocabulaire au profil de l'utilisateur\n");
            }
        }

        // Remaining context (extras + non-mentioned schema fields), capped.
        let mentioned = [
            "name",
            "role",
            "domain.sector",
            "domain.team_size",
            "tech.proficiency",
            "preferences.language",
            "agents.hitl",
            "constraints.sovereignty",
            "constraints.compliance",
            "tools.daily",
            "goals",
        ];
        let remaining: Vec<&ProfileEntry> = entries
            .iter()
            .filter(|e| {
                !mentioned.iter().any(|k| *k == e.key)
                    && !e.key.starts_with("goal")
                    && !e.key.starts_with("tools")
            })
            .take(max_entries)
            .collect();
        if !remaining.is_empty() {
            output.push_str("\nContexte :\n");
            for entry in remaining {
                let stale_marker = if Self::is_stale(&entry.updated_at) {
                    " (peut-être obsolète)"
                } else {
                    ""
                };
                output.push_str(&format!(
                    "- {}: {}{stale_marker}\n",
                    entry.key, entry.value
                ));
            }
        }

        Ok(output)
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

    // -- Legacy API (deprecated, ADR-087) --

    /// Stores or updates a user memory entry with the default confidence.
    ///
    /// **Deprecated (ADR-087)**: prefer [`Self::set`].  The `category`
    /// argument is ignored — keys are stored flat.  `user.`-prefixed legacy
    /// keys are accepted and stripped to maintain backwards compatibility.
    pub fn store(
        &self,
        _category: UserMemoryCategory,
        key: &str,
        value: &str,
        source: UserMemorySource,
    ) -> Result<(), UserMemoryError> {
        let flat_key = Self::strip_user_prefix(key);
        self.set(flat_key, value, source.into_written_by())
    }

    /// Stores or updates a user memory entry with a custom confidence score.
    ///
    /// **Deprecated (ADR-087)**: the `confidence` argument is ignored; the
    /// simplified model always stores at confidence `1.0`.
    pub fn store_with_confidence(
        &self,
        _category: UserMemoryCategory,
        key: &str,
        value: &str,
        source: UserMemorySource,
        _confidence: f64,
    ) -> Result<(), UserMemoryError> {
        let flat_key = Self::strip_user_prefix(key);
        self.set(flat_key, value, source.into_written_by())
    }

    /// Returns entries from the profile, ignoring the `_category` filter.
    ///
    /// **Deprecated (ADR-087)**: prefer [`Self::list_all`] /
    /// [`Self::list_schema`].  Returned entries have their `category` field
    /// best-effort derived from the canonical schema section.
    pub fn recall(
        &self,
        _category: UserMemoryCategory,
        limit: usize,
    ) -> Result<Vec<UserMemoryEntry>, UserMemoryError> {
        let entries = self.list_all()?;
        Ok(entries
            .into_iter()
            .take(limit)
            .map(Self::profile_to_legacy)
            .collect())
    }

    /// Looks up a single entry by key (category ignored).
    ///
    /// **Deprecated (ADR-087)**: prefer [`Self::get`].
    pub fn recall_by_key(
        &self,
        _category: UserMemoryCategory,
        key: &str,
    ) -> Result<Option<UserMemoryEntry>, UserMemoryError> {
        let flat_key = Self::strip_user_prefix(key);
        Ok(self.get(flat_key)?.map(Self::profile_to_legacy))
    }

    /// Updates only the confidence score of an existing entry.
    ///
    /// **Deprecated (ADR-087)**: the confidence model is removed in the
    /// simplified API.  This method is a no-op except for verifying the entry
    /// exists, returning [`UserMemoryError::NotFound`] otherwise.
    pub fn update_confidence(&self, key: &str, _confidence: f64) -> Result<(), UserMemoryError> {
        let flat_key = Self::strip_user_prefix(key);
        if self.get(flat_key)?.is_none() {
            return Err(UserMemoryError::NotFound(key.to_owned()));
        }
        Ok(())
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

    /// Strips the legacy `user.` prefix when present.  Returns the input
    /// untouched otherwise.
    fn strip_user_prefix(key: &str) -> &str {
        key.strip_prefix("user.").unwrap_or(key)
    }

    /// Writes an entry without external-key validation — used by both the
    /// canonical API (after validation) and internal-state setters.
    fn write_raw(
        &self,
        key: &str,
        value: &str,
        written_by: WrittenBy,
    ) -> Result<(), UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let json_value = serde_json::Value::String(value.to_owned());
        sem.remember(
            USER_NAMESPACE,
            key,
            &json_value,
            1.0,
            Some(&written_by.tag()),
            None,
        )
        .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Converts a [`SemanticEntry`] to a canonical [`ProfileEntry`].
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
            .unwrap_or_else(|| WrittenBy::Agent("legacy".to_owned()));
        ProfileEntry {
            key: se.key.clone(),
            value,
            written_by,
            created_at: se.created_at.clone(),
            updated_at: se.updated_at.clone(),
            in_schema: is_canonical(&se.key),
        }
    }

    /// Converts a canonical [`ProfileEntry`] into a legacy
    /// [`UserMemoryEntry`].
    fn profile_to_legacy(entry: ProfileEntry) -> UserMemoryEntry {
        let category = field_for(&entry.key)
            .map(|f| match f.section {
                crate::profile_schema::ProfileSection::Identity => UserMemoryCategory::Context,
                crate::profile_schema::ProfileSection::Work => UserMemoryCategory::Context,
                crate::profile_schema::ProfileSection::Preferences => {
                    UserMemoryCategory::Preferences
                }
                crate::profile_schema::ProfileSection::Constraints => {
                    UserMemoryCategory::Preferences
                }
            })
            .unwrap_or(UserMemoryCategory::Context);
        let source = match entry.written_by {
            WrittenBy::Onboarding => UserMemorySource::Onboarding,
            WrittenBy::User => UserMemorySource::UserExplicit,
            WrittenBy::Agent(ref name) if name == "chat-extractor" => {
                UserMemorySource::ChatInference
            }
            WrittenBy::Agent(_) => UserMemorySource::AgentObservation,
        };
        UserMemoryEntry {
            category,
            key: entry.key,
            value: entry.value,
            source,
            confidence: 1.0,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (UserMemoryRepository, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_user_mem_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user_memory.db");
        let repo = UserMemoryRepository::new(&path).unwrap();
        (repo, path)
    }

    #[test]
    fn set_and_get_roundtrip() {
        let (repo, _) = setup();
        repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
        let entry = repo.get("name").unwrap().expect("entry should exist");
        assert_eq!(entry.value, "Nidal");
        assert_eq!(entry.written_by, WrittenBy::Onboarding);
        assert!(entry.in_schema);
    }

    #[test]
    fn list_schema_and_extras_are_partitioned() {
        let (repo, _) = setup();
        repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
        repo.set("favorite_color", "blue", WrittenBy::User).unwrap();

        let schema = repo.list_schema().unwrap();
        let extras = repo.list_extras().unwrap();
        assert!(schema.iter().any(|e| e.key == "name"));
        assert!(!schema.iter().any(|e| e.key == "favorite_color"));
        assert!(extras.iter().any(|e| e.key == "favorite_color"));
        assert!(!extras.iter().any(|e| e.key == "name"));
    }

    #[test]
    fn set_rejects_internal_keys() {
        let (repo, _) = setup();
        let err = repo
            .set("__sneaky", "x", WrittenBy::User)
            .expect_err("internal keys must be rejected");
        assert!(matches!(err, UserMemoryError::InvalidKey(_)));
    }

    #[test]
    fn reset_preserves_internal_state() {
        let (repo, _) = setup();
        repo.set("name", "Nidal", WrittenBy::User).unwrap();
        repo.mark_topic_covered("identity").unwrap();
        repo.set_onboarding_skipped(true).unwrap();

        let removed = repo.reset().unwrap();
        assert_eq!(removed, 1, "only the user-visible entry should be removed");
        assert!(repo.get("name").unwrap().is_none());
        assert!(repo.get_onboarding_skipped().unwrap());
        assert_eq!(repo.get_covered_topics().unwrap(), vec!["identity"]);
    }

    #[test]
    fn list_orders_schema_then_extras() {
        let (repo, _) = setup();
        repo.set("zzz_extra", "x", WrittenBy::User).unwrap();
        repo.set("role", "CTO", WrittenBy::User).unwrap();
        repo.set("aaa_extra", "y", WrittenBy::User).unwrap();
        repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();

        let all = repo.list_all().unwrap();
        assert_eq!(all[0].key, "name");
        assert_eq!(all[1].key, "role");
        let extras_keys: Vec<&str> = all
            .iter()
            .filter(|e| !e.in_schema)
            .map(|e| e.key.as_str())
            .collect();
        assert_eq!(extras_keys, vec!["aaa_extra", "zzz_extra"]);
    }

    #[test]
    fn legacy_store_drops_category_and_user_prefix() {
        let (repo, _) = setup();
        repo.store(
            UserMemoryCategory::Preferences,
            "user.name",
            "Nidal",
            UserMemorySource::Onboarding,
        )
        .unwrap();
        let entry = repo.get("name").unwrap().expect("entry should exist");
        assert_eq!(entry.value, "Nidal");
        assert_eq!(entry.written_by, WrittenBy::Onboarding);
    }

    #[test]
    fn legacy_recall_returns_all_entries() {
        let (repo, _) = setup();
        repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
        repo.set("role", "CTO", WrittenBy::User).unwrap();

        let legacy = repo.recall(UserMemoryCategory::Preferences, 10).unwrap();
        assert_eq!(legacy.len(), 2);
        let keys: Vec<&str> = legacy.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"role"));
    }

    #[test]
    fn forget_returns_not_found_when_missing() {
        let (repo, _) = setup();
        let err = repo.forget("nonexistent").expect_err("must fail");
        assert!(matches!(err, UserMemoryError::NotFound(_)));
    }

    #[test]
    fn forget_removes_entry() {
        let (repo, _) = setup();
        repo.set("name", "Nidal", WrittenBy::User).unwrap();
        repo.forget("name").unwrap();
        assert!(repo.get("name").unwrap().is_none());
    }

    #[test]
    fn update_preserves_provenance() {
        let (repo, _) = setup();
        repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
        repo.update("name", "Alice").unwrap();
        let entry = repo.get("name").unwrap().unwrap();
        assert_eq!(entry.value, "Alice");
        assert_eq!(entry.written_by, WrittenBy::Onboarding);
    }

    #[test]
    fn recall_persona_brief_reflects_canonical_keys() {
        let (repo, _) = setup();
        repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
        repo.set("role", "CTO fintech", WrittenBy::Onboarding).unwrap();
        repo.set("agents.hitl", "critical-only", WrittenBy::Onboarding)
            .unwrap();
        repo.set(
            "constraints.sovereignty",
            "local-preferred",
            WrittenBy::Onboarding,
        )
        .unwrap();
        repo.set("domain.sector", "fintech", WrittenBy::User).unwrap();
        repo.set("tech.proficiency", "expert", WrittenBy::User).unwrap();

        let brief = repo.recall_persona_brief(20).unwrap();
        assert!(brief.contains("Nidal"), "brief should contain name: {brief}");
        assert!(brief.contains("CTO fintech"), "{brief}");
        assert!(
            brief.contains("supervision sur actions critiques"),
            "{brief}"
        );
        assert!(brief.contains("local préféré"), "{brief}");
        assert!(brief.contains("fintech"), "{brief}");
        assert!(brief.contains("expert"), "{brief}");
    }

    #[test]
    fn is_empty_on_fresh_and_after_set() {
        let (repo, _) = setup();
        assert!(repo.is_empty().unwrap());
        repo.set_onboarding_skipped(true).unwrap();
        assert!(
            repo.is_empty().unwrap(),
            "internal markers should not count as 'visible' entries"
        );
        repo.set("name", "Nidal", WrittenBy::User).unwrap();
        assert!(!repo.is_empty().unwrap());
    }

    #[test]
    fn search_returns_visible_entries() {
        let (repo, _) = setup();
        repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
        repo.set("preferences.language", "francais", WrittenBy::User)
            .unwrap();
        let results = repo.search("francais", 10).unwrap();
        assert!(results.iter().any(|e| e.key == "preferences.language"));
    }

    #[test]
    fn onboarding_topics_round_trip() {
        let (repo, _) = setup();
        repo.mark_topic_covered("identity").unwrap();
        repo.mark_topic_covered("preferences").unwrap();
        let topics = repo.get_covered_topics().unwrap();
        assert_eq!(topics.len(), 2);
        assert!(topics.contains(&"identity".to_string()));
        assert!(topics.contains(&"preferences".to_string()));
    }

    #[test]
    fn onboarding_session_round_trip() {
        let (repo, _) = setup();
        assert!(repo.get_last_onboarding_session().unwrap().is_none());
        repo.set_last_onboarding_session("2026-05-11T10:00:00Z")
            .unwrap();
        assert_eq!(
            repo.get_last_onboarding_session().unwrap().as_deref(),
            Some("2026-05-11T10:00:00Z")
        );
    }

    #[test]
    fn written_by_tag_round_trip() {
        assert_eq!(WrittenBy::Onboarding.tag(), "onboarding");
        assert_eq!(WrittenBy::User.tag(), "user");
        assert_eq!(
            WrittenBy::Agent("veille-ia".to_owned()).tag(),
            "agent:veille-ia"
        );
        assert_eq!(WrittenBy::from_tag("onboarding"), WrittenBy::Onboarding);
        assert_eq!(WrittenBy::from_tag("user"), WrittenBy::User);
        assert_eq!(
            WrittenBy::from_tag("agent:veille-ia"),
            WrittenBy::Agent("veille-ia".to_owned())
        );
        // Legacy mapping
        assert_eq!(WrittenBy::from_tag("user_explicit"), WrittenBy::User);
        assert_eq!(
            WrittenBy::from_tag("chat_inference"),
            WrittenBy::Agent("chat-extractor".to_owned())
        );
    }
}
