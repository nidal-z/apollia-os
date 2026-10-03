use super::*;

/// Open the trigger history persistence (`triggers.db`). `None` (logged) on failure.
pub(in crate::supervisor) fn open_trigger_persistence(
    data_dir: &std::path::Path,
) -> Option<TriggerPersistence> {
    match TriggerPersistence::open(
        &data_dir.join(apollia_core::paths::DataFile::Triggers.file_name()),
    ) {
        Ok(p) => {
            info!("supervisor.trigger_persistence.ready");
            Some(p)
        }
        Err(e) => {
            warn!(
                error = %e,
                detail = "trigger history disabled",
                "supervisor.trigger_persistence.failed"
            );
            None
        }
    }
}

/// Open the audit trail (`audit.db`). Returns `None` (logged) on failure.
pub(in crate::supervisor) async fn open_audit_trail(
    data_dir: &std::path::Path,
) -> Option<AuditTrailHandle> {
    match AuditTrailHandle::open(&data_dir.join(apollia_core::paths::DataFile::Audit.file_name()))
        .await
    {
        Ok(handle) => {
            info!("supervisor.audit_trail.ready");
            Some(handle)
        }
        Err(e) => {
            warn!(error = %e, detail = "audit disabled", "supervisor.audit_trail.failed");
            None
        }
    }
}

/// Open the hash-chained, signed audit journal (`audit_journal.db`).
///
/// The HMAC signing key lives in a local file under the data dir (see
/// [`load_or_create_journal_key`]), generated on first boot. A local file
/// rather than the OS keychain so a dev rebuild never blocks startup on a
/// keychain-access prompt. Falls back to an unsigned journal (logged) if the
/// key is unavailable, so the hash chain and `audit verify` keep working.
pub(in crate::supervisor) async fn open_audit_journal(
    data_dir: &std::path::Path,
) -> Option<AuditJournalHandle> {
    let db_path = data_dir.join(apollia_core::paths::DataFile::AuditJournal.file_name());
    match load_or_create_journal_key(data_dir) {
        Some(key) => match AuditJournalHandle::open_with_key_bytes(&db_path, key).await {
            Ok(handle) => {
                info!("supervisor.audit_journal.ready");
                Some(handle)
            }
            Err(e) => {
                // Signing is lost: the journal opens unsigned, so its
                // tamper-evidence drops to the keyless hash chain. Surface it as
                // a stable, greppable event, not a passing note.
                warn!(error = %e, "audit.journal.unsigned_fallback");
                open_unsigned_journal(&db_path).await
            }
        },
        None => {
            warn!("audit.journal.unsigned_fallback");
            open_unsigned_journal(&db_path).await
        }
    }
}

/// Open the journal without a signer. Returns `None` (logged) on failure.
async fn open_unsigned_journal(db_path: &std::path::Path) -> Option<AuditJournalHandle> {
    match AuditJournalHandle::open(db_path).await {
        Ok(handle) => Some(handle),
        Err(e) => {
            warn!(error = %e, detail = "journal disabled", "supervisor.audit_journal.failed");
            None
        }
    }
}

/// Load the journal HMAC key from `<data_dir>/journal-hmac-key`, generating and
/// persisting a random 32-byte key (base64, `0600`) on first boot.
///
/// A local key file (scope local-only) rather than the OS keychain: reading a
/// keychain entry written by a different binary signature (every unsigned dev
/// rebuild) blocks the daemon on a `SecurityAgent` prompt. Returns the key
/// bytes; `None` only when a key can neither be read nor generated.
fn load_or_create_journal_key(data_dir: &std::path::Path) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let key_path = data_dir.join("journal-hmac-key");

    if let Ok(contents) = std::fs::read_to_string(&key_path) {
        match base64::engine::general_purpose::STANDARD.decode(contents.trim()) {
            Ok(bytes) if !bytes.is_empty() => return Some(bytes),
            _ => warn!(detail = "regenerating", "audit.journal.key.unreadable"),
        }
    }

    let mut key = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut key);
    let encoded = base64::engine::general_purpose::STANDARD.encode(key);
    if let Err(e) = write_key_file(&key_path, encoded.as_bytes()) {
        // Sign this session with the in-memory key even if it cannot be
        // persisted (rare: data dir not writable, in which case the db write
        // would also fail).
        warn!(error = %e, "audit.journal.key.persist.failed");
        return Some(key.to_vec());
    }
    info!("audit.journal.key.generated");
    Some(key.to_vec())
}

/// Write the signing key to a fresh file that is owner-only from creation.
///
/// On unix the file is created with mode `0600` before any byte is written, so
/// the key material never lands on disk under a broader mode (the earlier
/// write-then-chmod left a short world-readable window). Any prior key file
/// (including an unreadable one being regenerated) is removed first so the
/// `create_new` open always yields a fresh owner-only file. Non-unix relies on
/// default filesystem ACLs; macOS, the primary desktop, is unix.
fn write_key_file(key_path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    match std::fs::remove_file(key_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut file = opts.open(key_path)?;
    file.write_all(contents)?;
    file.flush()
}

/// Open the HITL task repository (`hitl.db`). Returns `None` (logged) on failure.
pub(in crate::supervisor) async fn open_task_repository(
    data_dir: &std::path::Path,
) -> Option<Arc<TaskRepository>> {
    match TaskRepository::open(&data_dir.join(apollia_core::paths::DataFile::Hitl.file_name()))
        .await
    {
        Ok(repo) => {
            info!("supervisor.task_repository.ready");
            Some(Arc::new(repo))
        }
        Err(e) => {
            warn!(error = %e, detail = "HITL disabled", "supervisor.task_repository.failed");
            None
        }
    }
}

/// Open the user memory repository (`user_memory.db`). `None` (logged) on failure.
pub(in crate::supervisor) fn open_user_memory(
    data_dir: &std::path::Path,
) -> Option<std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>> {
    match apollia_memory::user_memory::UserMemoryRepository::new(
        &data_dir.join(apollia_core::paths::DataFile::UserMemory.file_name()),
    ) {
        Ok(repo) => {
            migrate_legacy_user_profile(data_dir, &repo);
            info!("supervisor.user_memory.ready");
            Some(std::sync::Arc::new(std::sync::Mutex::new(repo)))
        }
        Err(e) => {
            warn!(error = %e, detail = "user memory disabled", "supervisor.user_memory.failed");
            None
        }
    }
}

/// One-time migration of the historical `ctx.profile` store.
///
/// Before the profile write path was unified, agent writes landed in a separate
/// `memory/__user__.db` while the desktop and CLI read `user_memory.db`. This
/// copies any entry from the legacy file into the canonical repository (without
/// clobbering an existing key, so the canonical value always wins), then retires
/// the legacy file so the migration runs only once. Every failure is non-fatal
/// and logged.
fn migrate_legacy_user_profile(
    data_dir: &std::path::Path,
    canonical: &apollia_memory::user_memory::UserMemoryRepository,
) {
    let legacy_path = data_dir.join("memory").join("__user__.db");
    if !legacy_path.exists() {
        return;
    }
    let legacy = match apollia_memory::user_memory::UserMemoryRepository::new(&legacy_path) {
        Ok(repo) => repo,
        Err(e) => {
            warn!(
                error = %e,
                detail = "skipping the profile migration",
                "memory.legacy.unreadable"
            );
            return;
        }
    };
    let entries = match legacy.list_all() {
        Ok(entries) => entries,
        Err(e) => {
            warn!(
                error = %e,
                detail = "skipping the profile migration",
                "memory.legacy.read.failed"
            );
            return;
        }
    };
    let mut migrated = 0usize;
    for entry in entries {
        match canonical.get(&entry.key) {
            Ok(Some(_)) => continue,
            Ok(None) => match canonical.set(&entry.key, &entry.value, entry.written_by) {
                Ok(()) => migrated += 1,
                Err(e) => {
                    warn!(key = %entry.key, error = %e, "memory.legacy.entry.migrate.failed")
                }
            },
            Err(e) => {
                warn!(key = %entry.key, error = %e, "memory.legacy.entry.probe.failed")
            }
        }
    }
    let retired = legacy_path.with_extension("db.migrated");
    if let Err(e) = std::fs::rename(&legacy_path, &retired) {
        warn!(
            error = %e,
            detail = "entries migrated,
            the old file stays",
            "memory.legacy.retire.failed"
        );
    }
    if migrated > 0 {
        info!(count = migrated, "memory.legacy.migrated");
    }
}

/// Open the plan cache repository (`plan_cache.db`). `None` (logged) on failure.
pub(in crate::supervisor) fn open_plan_cache(
    data_dir: &std::path::Path,
) -> Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>> {
    match apollia_oria::plan_cache::PlanCacheRepository::open(
        &data_dir.join(apollia_core::paths::DataFile::PlanCache.file_name()),
    ) {
        Ok(repo) => {
            info!("supervisor.plan_cache.ready");
            Some(Arc::new(std::sync::Mutex::new(repo)))
        }
        Err(e) => {
            warn!(error = %e, detail = "plan caching disabled", "supervisor.plan_cache.failed");
            None
        }
    }
}

/// Open the sidechain repository (`sidechains.db`) and wrap it in a logger.
/// Returns `None` (logged) on failure.
pub(in crate::supervisor) fn open_sidechain_logger(
    data_dir: &std::path::Path,
) -> Option<crate::a2a::SidechainLogger> {
    match crate::a2a::SidechainRepository::open(
        &data_dir.join(apollia_core::paths::DataFile::Sidechains.file_name()),
    ) {
        Ok(repo) => {
            info!("supervisor.sidechain.ready");
            Some(crate::a2a::SidechainLogger::new(std::sync::Arc::new(
                std::sync::Mutex::new(repo),
            )))
        }
        Err(e) => {
            warn!(error = %e, detail = "sidechain logging disabled", "supervisor.sidechain.failed");
            None
        }
    }
}

/// Phase 13b: open `projects.db` and seed the built-in project templates.
/// Returns `None` (logged) when the database cannot be opened.
pub(in crate::supervisor) fn open_project_repository(
    data_dir: &std::path::Path,
) -> Option<std::sync::Arc<apollia_tools::ProjectRepository>> {
    let db_path = data_dir.join(apollia_core::paths::DataFile::Projects.file_name());
    match apollia_tools::ProjectRepository::open(&db_path) {
        Ok(repo) => {
            if let Err(e) = repo.seed_builtin_templates() {
                warn!(error = %e, "project.templates.seed.failed");
            }
            info!("supervisor.projects.ready");
            Some(std::sync::Arc::new(repo))
        }
        Err(e) => {
            warn!(error = %e, detail = "projects disabled", "supervisor.projects.failed");
            None
        }
    }
}

/// Resolves `~` at the start of a path to `$HOME`.
pub(crate) fn resolve_home(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = apollia_core::paths::home_string() {
            return std::path::PathBuf::from(home).join(stripped);
        }
    }
    path.to_owned()
}
