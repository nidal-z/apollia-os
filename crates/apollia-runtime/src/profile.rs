//! The user profile as every surface reads and writes it.
//!
//! The profile lives in `user_memory.db`, behind [`UserMemoryRepository`].
//! The desktop running the runtime in its own process calls these functions
//! directly; a client of the daemon reaches the same functions through
//! `/api/v1/profile`. Both therefore answer the same view and refuse with the
//! same words.

use std::sync::{Arc, Mutex};

use apollia_memory::user_memory::{ProfileEntry, UserMemoryError, UserMemoryRepository, WrittenBy};
use serde::{Deserialize, Serialize};

/// The profile repository as the runtime already shares it, borrowed for one
/// operation. The lock is the one `user_memory.db` was opened behind at
/// startup; this type adds no shared state of its own.
#[derive(Clone, Copy)]
pub struct ProfileStore<'a>(pub &'a Arc<Mutex<UserMemoryRepository>>);

/// One profile entry, as the profile page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProfileEntryView {
    /// Flat storage key.
    pub key: String,
    /// Plain-text value.
    pub value: String,
    /// Provenance tag (`onboarding`, `user`, `agent:<name>`).
    pub written_by: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
    /// `true` when the key is a field of the canonical profile schema.
    pub in_schema: bool,
}

/// The whole profile: the schema fields, then the free-form entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UserProfileView {
    /// Entries whose key is a field of the schema.
    pub schema_entries: Vec<ProfileEntryView>,
    /// Entries outside the schema.
    pub extras: Vec<ProfileEntryView>,
    /// Every entry, in the order the repository lists them.
    pub entries: Vec<ProfileEntryView>,
    /// The most recent `updated_at` across the entries.
    pub last_updated_at: Option<String>,
}

/// Why a profile operation did not complete. The display text is what the
/// interface shows, the same whichever process ran the operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProfileError {
    /// `user_memory.db` did not open at startup.
    #[error("NOT_INITIALIZED: User profile repository is not initialized")]
    Unavailable,
    /// The key is empty.
    #[error("VALIDATION_ERROR: Key must not be empty")]
    EmptyKey,
    /// No entry has this key.
    #[error("NOT_FOUND: Profile entry not found: {0}")]
    NotFound(String),
    /// The repository refused the operation.
    #[error("{0}")]
    Storage(#[source] UserMemoryError),
    /// The entry just written could not be read back.
    #[error("entry '{0}' missing after store")]
    MissingAfterWrite(String),
    /// A writer panicked while holding the repository.
    #[error("mutex poisoned")]
    Poisoned,
    /// The blocking task did not complete.
    #[error("spawn_blocking failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}

fn view(entry: &ProfileEntry) -> ProfileEntryView {
    ProfileEntryView {
        key: entry.key.clone(),
        value: entry.value.clone(),
        written_by: entry.written_by.tag(),
        created_at: entry.created_at.clone(),
        updated_at: entry.updated_at.clone(),
        in_schema: entry.in_schema,
    }
}

/// Runs `operation` on the repository off the async threads.
async fn with_repo<T, F>(store: ProfileStore<'_>, operation: F) -> Result<T, ProfileError>
where
    T: Send + 'static,
    F: FnOnce(&UserMemoryRepository) -> Result<T, ProfileError> + Send + 'static,
{
    let repo = Arc::clone(store.0);
    tokio::task::spawn_blocking(move || {
        let guard = repo.lock().map_err(|_| ProfileError::Poisoned)?;
        operation(&guard)
    })
    .await
    .map_err(ProfileError::Join)?
}

/// Reads the whole profile.
///
/// # Errors
///
/// [`ProfileError::Storage`] when the repository cannot be read.
pub async fn read_profile(store: ProfileStore<'_>) -> Result<UserProfileView, ProfileError> {
    let entries = with_repo(store, |repo| repo.list_all().map_err(ProfileError::Storage)).await?;
    let entries: Vec<ProfileEntryView> = entries.iter().map(view).collect();
    Ok(UserProfileView {
        schema_entries: entries.iter().filter(|e| e.in_schema).cloned().collect(),
        extras: entries.iter().filter(|e| !e.in_schema).cloned().collect(),
        last_updated_at: entries.iter().map(|e| e.updated_at.clone()).max(),
        entries,
    })
}

/// Creates or replaces one entry, written by the user, and returns it as
/// stored.
///
/// # Errors
///
/// [`ProfileError::EmptyKey`] for an empty key, [`ProfileError::Storage`] when
/// the repository refuses the write.
pub async fn write_entry(
    store: ProfileStore<'_>,
    key: String,
    value: String,
) -> Result<ProfileEntryView, ProfileError> {
    if key.is_empty() {
        return Err(ProfileError::EmptyKey);
    }
    let stored = with_repo(store, move |repo| {
        repo.set(&key, &value, WrittenBy::User)
            .map_err(ProfileError::Storage)?;
        repo.get(&key)
            .map_err(ProfileError::Storage)?
            .ok_or(ProfileError::MissingAfterWrite(key))
    })
    .await?;
    Ok(view(&stored))
}

/// Deletes one entry.
///
/// # Errors
///
/// [`ProfileError::NotFound`] when no entry has `key`.
pub async fn delete_entry(store: ProfileStore<'_>, key: String) -> Result<(), ProfileError> {
    with_repo(store, move |repo| match repo.forget(&key) {
        Ok(()) => Ok(()),
        Err(UserMemoryError::NotFound(_)) => Err(ProfileError::NotFound(key)),
        Err(other) => Err(ProfileError::Storage(other)),
    })
    .await
}

/// Deletes every entry the user sees, keeping the onboarding markers, and
/// returns how many were removed.
///
/// # Errors
///
/// [`ProfileError::Storage`] when the repository refuses the deletion.
pub async fn reset_profile(store: ProfileStore<'_>) -> Result<usize, ProfileError> {
    with_repo(store, |repo| repo.reset().map_err(ProfileError::Storage)).await
}
