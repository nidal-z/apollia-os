//! UserMemoryRepository — global user memory backed by SemanticMemory.
//!
//! Persists user preferences, habits, and contextual information under
//! the reserved `__user__` namespace.  The repository delegates storage
//! to [`SemanticMemory`] and [`MemorySearch`], adding domain-specific
//! typing (category, source) and an LLM-friendly recall format.

use std::fmt;
use std::path::Path;

use crate::search::{MemorySearch, SearchSource};
use crate::semantic::SemanticMemory;
use crate::store::MemoryStore;

/// Reserved SemanticMemory namespace for user-level data.
const USER_NAMESPACE: &str = "__user__";

/// Default confidence score assigned to user memory entries.
const DEFAULT_CONFIDENCE: f64 = 1.0;

/// Separator between the category prefix and the key in semantic storage.
const KEY_SEPARATOR: char = '.';

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Category of a user memory entry (ADR-038).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryCategory {
    /// User profile identity (name, role, industry, goals, etc.).
    Profile,
    /// User preferences (language, output format, tone, etc.).
    Preferences,
    /// Observed user habits (working hours, review frequency, etc.).
    Habits,
    /// Contextual information (current project, role, team, etc.).
    Context,
}

impl UserMemoryCategory {
    /// Returns the string tag used as key prefix in SemanticMemory.
    fn as_str(&self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Preferences => "preferences",
            Self::Habits => "habits",
            Self::Context => "context",
        }
    }

    /// Ordered list of all categories, used for grouped recall.
    fn all() -> &'static [Self] {
        &[
            Self::Profile,
            Self::Preferences,
            Self::Habits,
            Self::Context,
        ]
    }

    /// Parses a category from its string tag.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "profile" => Some(Self::Profile),
            "preferences" => Some(Self::Preferences),
            "habits" => Some(Self::Habits),
            "context" => Some(Self::Context),
            _ => None,
        }
    }
}

impl fmt::Display for UserMemoryCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Source of a user memory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemorySource {
    /// Set during first-run onboarding wizard.
    Onboarding,
    /// Inferred by the LLM from a chat conversation.
    ChatInference,
    /// Explicitly provided by the user.
    UserExplicit,
    /// Observed by an agent during task execution.
    AgentObservation,
}

impl UserMemorySource {
    /// Returns the string tag stored in SemanticMemory's `source` field.
    fn as_str(&self) -> &'static str {
        match self {
            Self::Onboarding => "onboarding",
            Self::ChatInference => "chat_inference",
            Self::UserExplicit => "user_explicit",
            Self::AgentObservation => "agent_observation",
        }
    }

    /// Parses a source from its string tag.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "onboarding" => Some(Self::Onboarding),
            "chat_inference" => Some(Self::ChatInference),
            "user_explicit" => Some(Self::UserExplicit),
            "agent_observation" => Some(Self::AgentObservation),
            _ => None,
        }
    }
}

impl fmt::Display for UserMemorySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single user memory entry returned by recall operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserMemoryEntry {
    /// Category of the entry.
    pub category: UserMemoryCategory,
    /// Short key identifying the entry within its category.
    pub key: String,
    /// Human-readable value.
    pub value: String,
    /// How this entry was created.
    pub source: UserMemorySource,
    /// Confidence score (0.0..=1.0).
    pub confidence: f64,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

/// Errors from [`UserMemoryRepository`] operations.
#[derive(Debug, thiserror::Error)]
pub enum UserMemoryError {
    /// A storage operation failed.
    #[error("storage error: {0}")]
    StorageError(String),
    /// The provided category string is not recognized.
    #[error("invalid category: {0}")]
    InvalidCategory(String),
    /// The requested entry was not found.
    #[error("entry not found: {0}")]
    NotFound(String),
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

/// Repository for global user memory backed by [`SemanticMemory`].
///
/// All entries live under the `__user__` namespace in a shared
/// [`MemoryStore`].  Keys are prefixed with the category tag
/// (`preferences.language`, `habits.working_hours`, etc.).
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

    /// Returns `true` if the repository contains no user memory entries.
    pub fn is_empty(&self) -> Result<bool, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        Ok(all.is_empty())
    }

    /// Stores or updates a user memory entry with the default confidence (1.0).
    ///
    /// If an entry with the same `key` already exists in `category`,
    /// the value is replaced (upsert semantics).
    pub fn store(
        &self,
        category: UserMemoryCategory,
        key: &str,
        value: &str,
        source: UserMemorySource,
    ) -> Result<(), UserMemoryError> {
        self.store_with_confidence(category, key, value, source, DEFAULT_CONFIDENCE)
    }

    /// Stores or updates a user memory entry with a custom confidence score.
    ///
    /// If an entry with the same `key` already exists in `category`,
    /// the value is replaced (upsert semantics).
    pub fn store_with_confidence(
        &self,
        category: UserMemoryCategory,
        key: &str,
        value: &str,
        source: UserMemorySource,
        confidence: f64,
    ) -> Result<(), UserMemoryError> {
        let compound_key = Self::compound_key(category, key);
        let sem = SemanticMemory::new(&self.store);
        let json_value = serde_json::Value::String(value.to_owned());

        sem.remember(
            USER_NAMESPACE,
            &compound_key,
            &json_value,
            confidence,
            Some(source.as_str()),
            None,
        )
        .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        Ok(())
    }

    /// Recalls entries for a given category, ordered by key, up to `limit`.
    pub fn recall(
        &self,
        category: UserMemoryCategory,
        limit: usize,
    ) -> Result<Vec<UserMemoryEntry>, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let prefix = format!("{}{KEY_SEPARATOR}", category.as_str());

        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        let entries: Vec<UserMemoryEntry> = all
            .into_iter()
            .filter(|e| e.key.starts_with(&prefix))
            .take(limit)
            .filter_map(|e| Self::semantic_to_entry(&e.key, &e))
            .collect();

        Ok(entries)
    }

    /// Looks up a single entry by category and key.
    ///
    /// Returns `None` if no entry matches the given category/key pair.
    pub fn recall_by_key(
        &self,
        category: UserMemoryCategory,
        key: &str,
    ) -> Result<Option<UserMemoryEntry>, UserMemoryError> {
        let compound_key = Self::compound_key(category, key);
        let sem = SemanticMemory::new(&self.store);

        let entry = sem
            .recall(USER_NAMESPACE, &compound_key)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        Ok(entry.and_then(|se| Self::semantic_to_entry(&compound_key, &se)))
    }

    /// Full-text search across all categories.
    ///
    /// Returns entries matching `query` via FTS5 BM25 ranking.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UserMemoryEntry>, UserMemoryError> {
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

        let mut entries = Vec::with_capacity(results.len());
        for result in &results {
            let maybe = sem
                .recall_all(USER_NAMESPACE, None)
                .map_err(|e| UserMemoryError::StorageError(e.to_string()))?
                .into_iter()
                .find(|e| e.id == result.source_id);

            if let Some(se) = maybe {
                if let Some(entry) = Self::semantic_to_entry(&se.key, &se) {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }

    /// Produces a text block suitable for LLM system-prompt injection.
    ///
    /// Entries are grouped by category. Output format:
    /// ```text
    /// Category: preferences
    /// - language: francais
    /// - format: markdown
    /// Category: habits
    /// - working_hours: 9h-18h
    /// ```
    pub fn recall_all_for_injection(&self, max_entries: usize) -> Result<String, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        let mut output = String::new();
        let mut total = 0usize;

        for &cat in UserMemoryCategory::all() {
            let prefix = format!("{}{KEY_SEPARATOR}", cat.as_str());
            let entries_for_cat: Vec<_> =
                all.iter().filter(|e| e.key.starts_with(&prefix)).collect();

            if entries_for_cat.is_empty() {
                continue;
            }

            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!("Category: {}\n", cat.as_str()));

            for entry in entries_for_cat {
                if total >= max_entries {
                    break;
                }
                let short_key = &entry.key[prefix.len()..];
                let value = entry
                    .value
                    .as_str()
                    .map(|s| s.to_owned())
                    .unwrap_or_else(|| entry.value.to_string());
                output.push_str(&format!("- {short_key}: {value}\n"));
                total += 1;
            }

            if total >= max_entries {
                break;
            }
        }

        Ok(output)
    }

    /// Produces a structured persona brief for LLM system-prompt injection.
    ///
    /// Unlike [`recall_all_for_injection`] which dumps raw key-values, this
    /// method generates a narrative persona summary with adaptation
    /// instructions.  Entries with `confidence < 0.3` are filtered out.
    ///
    /// Falls back gracefully when identity keys are missing — the narrative
    /// header simply omits unknown fields.
    pub fn recall_persona_brief(&self, max_entries: usize) -> Result<String, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        // Early return if no entries with sufficient confidence exist.
        let qualified: Vec<_> = all.iter().filter(|e| e.confidence >= 0.3).collect();
        if qualified.is_empty() {
            return Ok(String::new());
        }

        // Helper: find a value by scanning all entries for a compound key suffix.
        let find_value = |suffix: &str| -> Option<String> {
            all.iter()
                .find(|e| e.key.ends_with(suffix) && e.confidence >= 0.3)
                .and_then(|e| e.value.as_str().map(|s| s.to_owned()))
        };

        let mut output = String::new();

        // --- Narrative header ---
        // Suffix lookups support both legacy keys (`.industry`, `.expertise_level`)
        // and the canonical onboarding/Profile keys written today
        // (`.context.sector`, `.tech.proficiency`).
        let name = find_value(".name").or_else(|| find_value(".user_name"));
        let role = find_value(".role").or_else(|| find_value(".user_role"));
        let industry = find_value(".domain.sector").or_else(|| find_value(".industry"));
        let team_size = find_value(".domain.team_size");
        let expertise = find_value(".tech.proficiency").or_else(|| find_value(".expertise_level"));
        let language = find_value(".language");
        let verbosity = find_value(".verbosity");
        let tone = find_value(".tone");
        let hitl = find_value(".agents.hitl");
        let sovereignty = find_value(".constraints.sovereignty");
        let compliance = find_value(".constraints.compliance");

        // Build the persona headline.
        let mut headline_parts: Vec<String> = Vec::new();
        if let Some(ref n) = name {
            headline_parts.push(n.clone());
        }
        if let Some(ref r) = role {
            if headline_parts.is_empty() {
                headline_parts.push(format!("Role: {r}"));
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

        // --- Gouvernance (HITL, souveraineté, compliance) ---
        // Critical for agents adapting their tool-call strategy and refusal patterns.
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

        // --- Language & preferences ---
        let mut pref_parts: Vec<String> = Vec::new();
        if let Some(ref lang) = language {
            pref_parts.push(format!("Langue : {lang}"));
        }
        if let Some(ref v) = verbosity {
            pref_parts.push(format!("Réponses : {v}"));
        }
        if let Some(ref t) = tone {
            pref_parts.push(format!("Ton : {t}"));
        }
        if !pref_parts.is_empty() {
            output.push_str(&pref_parts.join(" | "));
            output.push('\n');
        }

        // --- Goals ---
        let goals: Vec<String> = all
            .iter()
            .filter(|e| e.key.contains("goal") && e.confidence >= 0.3)
            .filter_map(|e| e.value.as_str().map(|s| s.to_owned()))
            .collect();
        if !goals.is_empty() {
            output.push_str("\nObjectifs :\n");
            for g in &goals {
                output.push_str(&format!("- {g}\n"));
            }
        }

        // --- Daily tools ---
        let tools: Vec<String> = all
            .iter()
            .filter(|e| e.key.contains("tools") && e.confidence >= 0.3)
            .filter_map(|e| e.value.as_str().map(|s| s.to_owned()))
            .collect();
        if !tools.is_empty() {
            output.push_str("\nOutils : ");
            output.push_str(&tools.join(", "));
            output.push('\n');
        }

        // --- Adaptation instructions ---
        output.push_str("\nAdaptation :\n");
        if let Some(ref lang) = language {
            output.push_str(&format!("- Langue : {lang}\n"));
        }
        if let Some(ref v) = verbosity {
            output.push_str(&format!("- Profondeur : {v}\n"));
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

        // --- Remaining context (capped) ---
        let used_suffixes = [
            "name",
            "role",
            "industry",
            "expertise_level",
            "language",
            "verbosity",
            "tone",
            "domain.sector",
            "domain.team_size",
            "tech.proficiency",
            "agents.hitl",
            "constraints.sovereignty",
            "constraints.compliance",
        ];
        let remaining: Vec<_> = all
            .iter()
            .filter(|e| e.confidence >= 0.3)
            .filter(|e| {
                !used_suffixes.iter().any(|s| e.key.ends_with(s))
                    && !e.key.contains("goal")
                    && !e.key.contains("tools")
                    && !e.key.contains("onboarding_topic_")
                    && !e.key.contains("onboarding_skipped")
                    && !e.key.contains("onboarding_last_session")
                    && !e.key.contains("onboarding.state")
            })
            .take(max_entries)
            .collect();

        if !remaining.is_empty() {
            output.push_str("\nContexte :\n");
            for entry in remaining {
                let short_key = entry
                    .key
                    .split(KEY_SEPARATOR)
                    .next_back()
                    .unwrap_or(&entry.key);
                let value = entry
                    .value
                    .as_str()
                    .map(|s| s.to_owned())
                    .unwrap_or_else(|| entry.value.to_string());

                // Flag stale entries (older than ~6 months = 180 days).
                let stale_marker = if Self::is_stale(&entry.updated_at) {
                    " (peut-être obsolète)"
                } else {
                    ""
                };
                output.push_str(&format!("- {short_key}: {value}{stale_marker}\n"));
            }
        }

        Ok(output)
    }

    /// Returns `true` if `iso_ts` is older than 180 days from now.
    fn is_stale(iso_ts: &str) -> bool {
        // Best-effort: parse the date prefix (YYYY-MM-DD) and compare.
        let Some(date_str) = iso_ts.get(..10) else {
            return false;
        };
        let Ok(then) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            return false;
        };
        let now = chrono::Utc::now().date_naive();
        now.signed_duration_since(then).num_days() > 180
    }

    /// Removes the entry with the given `key` from any category.
    ///
    /// Returns `Ok(())` if the entry was found and deleted, or
    /// `Err(NotFound)` if no entry matched.
    pub fn forget(&self, key: &str) -> Result<(), UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);

        for &cat in UserMemoryCategory::all() {
            let compound = Self::compound_key(cat, key);
            let deleted = sem
                .forget(USER_NAMESPACE, &compound)
                .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

            if deleted {
                return Ok(());
            }
        }

        Err(UserMemoryError::NotFound(key.to_owned()))
    }

    /// Updates only the confidence score of an existing entry.
    ///
    /// The value and source are preserved.  Scans all categories for a
    /// matching key.  Returns `Err(NotFound)` if the key does not exist.
    pub fn update_confidence(&self, key: &str, confidence: f64) -> Result<(), UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);

        for &cat in UserMemoryCategory::all() {
            let compound = Self::compound_key(cat, key);
            let existing = sem
                .recall(USER_NAMESPACE, &compound)
                .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

            if let Some(entry) = existing {
                let source = entry
                    .source
                    .as_deref()
                    .and_then(UserMemorySource::from_str)
                    .unwrap_or(UserMemorySource::UserExplicit);

                let value = entry
                    .value
                    .as_str()
                    .map(|s| s.to_owned())
                    .unwrap_or_else(|| entry.value.to_string());

                self.store_with_confidence(cat, key, &value, source, confidence)?;
                return Ok(());
            }
        }

        Err(UserMemoryError::NotFound(key.to_owned()))
    }

    /// Updates the value of an existing entry, preserving its source.
    ///
    /// Scans all categories for a matching key.
    /// Returns `Err(NotFound)` if the key does not exist.
    pub fn update(&self, key: &str, value: &str) -> Result<(), UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);

        for &cat in UserMemoryCategory::all() {
            let compound = Self::compound_key(cat, key);
            let existing = sem
                .recall(USER_NAMESPACE, &compound)
                .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

            if let Some(entry) = existing {
                let source = entry
                    .source
                    .as_deref()
                    .and_then(UserMemorySource::from_str)
                    .unwrap_or(UserMemorySource::UserExplicit);

                self.store(cat, key, value, source)?;
                return Ok(());
            }
        }

        Err(UserMemoryError::NotFound(key.to_owned()))
    }

    // -- onboarding helpers --

    /// Returns the list of onboarding topics that have been covered.
    ///
    /// A topic is considered covered when a key `onboarding_topic_{topic}`
    /// exists in the `Context` category with source `Onboarding`.
    pub fn get_covered_topics(&self) -> Result<Vec<String>, UserMemoryError> {
        let sem = SemanticMemory::new(&self.store);
        let prefix = format!(
            "{}{KEY_SEPARATOR}onboarding_topic_",
            UserMemoryCategory::Context.as_str()
        );

        let all = sem
            .recall_all(USER_NAMESPACE, None)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;

        let topics: Vec<String> = all
            .into_iter()
            .filter(|e| e.key.starts_with(&prefix))
            .filter_map(|e| e.key.strip_prefix(&prefix).map(|t| t.to_owned()))
            .collect();

        Ok(topics)
    }

    /// Returns the ISO 8601 timestamp of the last onboarding session, if any.
    pub fn get_last_onboarding_session(&self) -> Result<Option<String>, UserMemoryError> {
        let entry = self.recall_by_key(UserMemoryCategory::Context, "onboarding_last_session")?;
        Ok(entry.map(|e| e.value))
    }

    /// Returns `true` if the user has explicitly dismissed (skipped) the onboarding.
    pub fn get_onboarding_skipped(&self) -> Result<bool, UserMemoryError> {
        let entry = self.recall_by_key(UserMemoryCategory::Context, "onboarding_skipped")?;
        Ok(entry.map(|e| e.value == "true").unwrap_or(false))
    }

    /// Marks or unmarks the onboarding as skipped.
    pub fn set_onboarding_skipped(&self, skipped: bool) -> Result<(), UserMemoryError> {
        self.store(
            UserMemoryCategory::Context,
            "onboarding_skipped",
            if skipped { "true" } else { "false" },
            UserMemorySource::Onboarding,
        )
    }

    /// Records that an onboarding topic has been covered.
    pub fn mark_topic_covered(&self, topic: &str) -> Result<(), UserMemoryError> {
        let key = format!("onboarding_topic_{topic}");
        self.store(
            UserMemoryCategory::Context,
            &key,
            "covered",
            UserMemorySource::Onboarding,
        )
    }

    /// Records the timestamp of the current onboarding session.
    pub fn set_last_onboarding_session(&self, timestamp: &str) -> Result<(), UserMemoryError> {
        self.store(
            UserMemoryCategory::Context,
            "onboarding_last_session",
            timestamp,
            UserMemorySource::Onboarding,
        )
    }

    // -- helpers --

    /// Builds the compound key stored in SemanticMemory (`category.key`).
    fn compound_key(category: UserMemoryCategory, key: &str) -> String {
        format!("{}{KEY_SEPARATOR}{key}", category.as_str())
    }

    /// Converts a [`SemanticEntry`] to a [`UserMemoryEntry`].
    ///
    /// Returns `None` if the key does not contain a valid category prefix.
    fn semantic_to_entry(
        compound_key: &str,
        se: &crate::semantic::SemanticEntry,
    ) -> Option<UserMemoryEntry> {
        let (cat_str, short_key) = compound_key.split_once(KEY_SEPARATOR)?;
        let category = UserMemoryCategory::from_str(cat_str)?;
        let source = se
            .source
            .as_deref()
            .and_then(UserMemorySource::from_str)
            .unwrap_or(UserMemorySource::UserExplicit);

        let value = se
            .value
            .as_str()
            .map(|s| s.to_owned())
            .unwrap_or_else(|| se.value.to_string());

        Some(UserMemoryEntry {
            category,
            key: short_key.to_owned(),
            value,
            source,
            confidence: se.confidence,
            created_at: se.created_at.clone(),
            updated_at: se.updated_at.clone(),
        })
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

    // Store + recall round-trip for each category
    #[test]
    fn test_store_recall_round_trip() {
        // GIVEN
        let (repo, _) = setup();

        // WHEN
        repo.store(
            UserMemoryCategory::Preferences,
            "language",
            "francais",
            UserMemorySource::UserExplicit,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Habits,
            "working_hours",
            "9h-18h",
            UserMemorySource::AgentObservation,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Context,
            "current_project",
            "apollia-os",
            UserMemorySource::Onboarding,
        )
        .unwrap();

        // THEN
        let prefs = repo.recall(UserMemoryCategory::Preferences, 10).unwrap();
        assert_eq!(prefs.len(), 1);
        assert_eq!(prefs[0].key, "language");
        assert_eq!(prefs[0].value, "francais");
        assert_eq!(prefs[0].source, UserMemorySource::UserExplicit);
        assert_eq!(prefs[0].category, UserMemoryCategory::Preferences);

        let habits = repo.recall(UserMemoryCategory::Habits, 10).unwrap();
        assert_eq!(habits.len(), 1);
        assert_eq!(habits[0].key, "working_hours");

        let ctx = repo.recall(UserMemoryCategory::Context, 10).unwrap();
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].key, "current_project");
    }

    // recall_all_for_injection produces correct grouped format
    #[test]
    fn test_recall_all_for_injection_format() {
        // GIVEN
        let (repo, _) = setup();
        repo.store(
            UserMemoryCategory::Preferences,
            "language",
            "francais",
            UserMemorySource::UserExplicit,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Preferences,
            "format",
            "markdown",
            UserMemorySource::UserExplicit,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Habits,
            "working_hours",
            "9h-18h",
            UserMemorySource::AgentObservation,
        )
        .unwrap();

        // WHEN
        let text = repo.recall_all_for_injection(50).unwrap();

        // THEN
        assert!(text.contains("Category: preferences"));
        assert!(text.contains("- language: francais"));
        assert!(text.contains("- format: markdown"));
        assert!(text.contains("Category: habits"));
        assert!(text.contains("- working_hours: 9h-18h"));
        // Preferences appear before habits (ordered by category)
        let pref_pos = text.find("Category: preferences").unwrap();
        let habit_pos = text.find("Category: habits").unwrap();
        assert!(pref_pos < habit_pos);
    }

    // FTS5 search returns results cross-categories
    #[test]
    fn test_search_fts5_cross_categories() {
        // GIVEN
        let (repo, _) = setup();
        repo.store(
            UserMemoryCategory::Preferences,
            "language",
            "francais",
            UserMemorySource::UserExplicit,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Context,
            "locale",
            "francais",
            UserMemorySource::Onboarding,
        )
        .unwrap();

        // WHEN
        let results = repo.search("francais", 10).unwrap();

        // THEN
        assert!(results.len() >= 2);
        let categories: Vec<_> = results.iter().map(|e| e.category).collect();
        assert!(categories.contains(&UserMemoryCategory::Preferences));
        assert!(categories.contains(&UserMemoryCategory::Context));
    }

    // Forget removes the entry
    #[test]
    fn test_forget_removes_entry() {
        // GIVEN
        let (repo, _) = setup();
        repo.store(
            UserMemoryCategory::Preferences,
            "language",
            "francais",
            UserMemorySource::UserExplicit,
        )
        .unwrap();

        // WHEN
        repo.forget("language").unwrap();

        // THEN
        let prefs = repo.recall(UserMemoryCategory::Preferences, 10).unwrap();
        assert!(prefs.is_empty());
    }

    // Update modifies the value
    #[test]
    fn test_update_modifies_value() {
        // GIVEN
        let (repo, _) = setup();
        repo.store(
            UserMemoryCategory::Preferences,
            "language",
            "francais",
            UserMemorySource::UserExplicit,
        )
        .unwrap();

        // WHEN
        repo.update("language", "english").unwrap();

        // THEN
        let prefs = repo.recall(UserMemoryCategory::Preferences, 10).unwrap();
        assert_eq!(prefs.len(), 1);
        assert_eq!(prefs[0].value, "english");
        assert_eq!(prefs[0].source, UserMemorySource::UserExplicit);
    }

    // Forget nonexistent key returns NotFound
    #[test]
    fn test_forget_nonexistent_returns_not_found() {
        // GIVEN
        let (repo, _) = setup();

        // WHEN
        let result = repo.forget("nonexistent");

        // THEN
        assert!(matches!(result, Err(UserMemoryError::NotFound(_))));
    }

    // Update nonexistent key returns NotFound
    #[test]
    fn test_update_nonexistent_returns_not_found() {
        // GIVEN
        let (repo, _) = setup();

        // WHEN
        let result = repo.update("nonexistent", "value");

        // THEN
        assert!(matches!(result, Err(UserMemoryError::NotFound(_))));
    }

    // recall_all_for_injection respects max_entries limit
    #[test]
    fn test_recall_all_for_injection_respects_limit() {
        // GIVEN
        let (repo, _) = setup();
        for i in 0..10 {
            repo.store(
                UserMemoryCategory::Preferences,
                &format!("key_{i}"),
                &format!("value_{i}"),
                UserMemorySource::UserExplicit,
            )
            .unwrap();
        }

        // WHEN
        let text = repo.recall_all_for_injection(3).unwrap();

        // THEN — at most 3 "- key:" lines
        let entry_count = text.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(entry_count, 3);
    }

    #[test]
    fn test_update_confidence_preserves_value_and_source() {
        // GIVEN an entry with confidence 0.5
        let (repo, _) = setup();
        repo.store_with_confidence(
            UserMemoryCategory::Preferences,
            "language",
            "francais",
            UserMemorySource::ChatInference,
            0.5,
        )
        .unwrap();

        // WHEN confidence is updated to 0.95
        repo.update_confidence("language", 0.95).unwrap();

        // THEN value and source are preserved, confidence is 0.95
        let entry = repo
            .recall_by_key(UserMemoryCategory::Preferences, "language")
            .unwrap()
            .expect("entry should exist");
        assert_eq!(entry.value, "francais");
        assert_eq!(entry.source, UserMemorySource::ChatInference);
        assert!((entry.confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn test_update_confidence_not_found() {
        // GIVEN an empty repo
        let (repo, _) = setup();

        // WHEN updating confidence for a non-existent key
        let result = repo.update_confidence("nonexistent", 0.95);

        // THEN NotFound is returned
        assert!(matches!(result, Err(UserMemoryError::NotFound(_))));
    }

    #[test]
    fn test_is_empty_on_fresh_repo() {
        // GIVEN a fresh repository with no entries
        let (repo, _) = setup();

        // WHEN / THEN
        assert!(repo.is_empty().unwrap(), "fresh repo should be empty");
    }

    #[test]
    fn test_is_empty_after_store() {
        // GIVEN a repository with one entry
        let (repo, _) = setup();
        repo.store(
            UserMemoryCategory::Preferences,
            "language",
            "fr",
            UserMemorySource::Onboarding,
        )
        .unwrap();

        // WHEN / THEN
        assert!(
            !repo.is_empty().unwrap(),
            "repo with entries should not be empty"
        );
    }

    #[test]
    fn test_get_covered_topics_empty() {
        // GIVEN a fresh repository
        let (repo, _) = setup();

        // WHEN
        let topics = repo.get_covered_topics().unwrap();

        // THEN
        assert!(topics.is_empty());
    }

    #[test]
    fn test_get_covered_topics_with_entries() {
        // GIVEN topics marked as covered
        let (repo, _) = setup();
        repo.mark_topic_covered("identity").unwrap();
        repo.mark_topic_covered("preferences").unwrap();
        repo.mark_topic_covered("tools").unwrap();

        // WHEN
        let topics = repo.get_covered_topics().unwrap();

        // THEN
        assert_eq!(topics.len(), 3);
        assert!(topics.contains(&"identity".to_string()));
        assert!(topics.contains(&"preferences".to_string()));
        assert!(topics.contains(&"tools".to_string()));
    }

    #[test]
    fn test_get_onboarding_skipped_default() {
        // GIVEN a fresh repository
        let (repo, _) = setup();

        // WHEN / THEN
        assert!(!repo.get_onboarding_skipped().unwrap());
    }

    #[test]
    fn test_set_and_get_onboarding_skipped() {
        // GIVEN a repository
        let (repo, _) = setup();

        // WHEN skipping onboarding
        repo.set_onboarding_skipped(true).unwrap();

        // THEN
        assert!(repo.get_onboarding_skipped().unwrap());
    }

    #[test]
    fn test_get_last_onboarding_session_default() {
        // GIVEN a fresh repository
        let (repo, _) = setup();

        // WHEN / THEN
        assert!(repo.get_last_onboarding_session().unwrap().is_none());
    }

    #[test]
    fn test_set_and_get_last_onboarding_session() {
        // GIVEN a repository
        let (repo, _) = setup();
        let timestamp = "2026-06-15T14:30:00Z";

        // WHEN
        repo.set_last_onboarding_session(timestamp).unwrap();

        // THEN
        assert_eq!(
            repo.get_last_onboarding_session().unwrap().as_deref(),
            Some(timestamp)
        );
    }

    #[test]
    fn test_dismiss_preserves_topics() {
        // GIVEN topics already covered
        let (repo, _) = setup();
        repo.mark_topic_covered("identity").unwrap();
        repo.mark_topic_covered("domain").unwrap();

        // WHEN dismissing onboarding
        repo.set_onboarding_skipped(true).unwrap();

        // THEN topics are preserved
        let topics = repo.get_covered_topics().unwrap();
        assert_eq!(topics.len(), 2);
        assert!(topics.contains(&"identity".to_string()));
        assert!(topics.contains(&"domain".to_string()));
    }

    // recall_persona_brief reflects the keys actually written by onboarding +
    // Profile (name, role, agents.hitl, constraints.sovereignty, context.sector,
    // tech.proficiency). Guards against the prior bug where the brief scanned
    // only orphan suffixes (.industry, .expertise_level) that nothing wrote.
    #[test]
    fn test_recall_persona_brief_reflects_onboarding_keys() {
        // GIVEN a repository populated as the onboarding agent + Profile do
        let (repo, _) = setup();
        repo.store(
            UserMemoryCategory::Context,
            "name",
            "Nidal",
            UserMemorySource::Onboarding,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Context,
            "role",
            "CTO startup fintech",
            UserMemorySource::Onboarding,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Habits,
            "agents.hitl",
            "critical-only",
            UserMemorySource::Onboarding,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Preferences,
            "constraints.sovereignty",
            "local-preferred",
            UserMemorySource::Onboarding,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Context,
            "domain.sector",
            "fintech",
            UserMemorySource::UserExplicit,
        )
        .unwrap();
        repo.store(
            UserMemoryCategory::Context,
            "tech.proficiency",
            "expert",
            UserMemorySource::UserExplicit,
        )
        .unwrap();

        // WHEN
        let brief = repo.recall_persona_brief(20).unwrap();

        // THEN the persona reflects all four canonical Tier 1 keys + Tier 2
        assert!(brief.contains("Nidal"), "brief should contain user name: {brief}");
        assert!(
            brief.contains("CTO startup fintech"),
            "brief should contain role: {brief}"
        );
        assert!(
            brief.contains("supervision sur actions critiques"),
            "brief should reflect agents.hitl=critical-only: {brief}"
        );
        assert!(
            brief.contains("local préféré"),
            "brief should reflect sovereignty=local-preferred: {brief}"
        );
        assert!(
            brief.contains("fintech"),
            "brief should mention sector fintech: {brief}"
        );
        assert!(
            brief.contains("expert"),
            "brief should mention tech proficiency: {brief}"
        );
    }
}
