use super::*;

/// Open the trigger history persistence (`triggers.db`). `None` (logged) on failure.
pub(in crate::supervisor) fn open_trigger_persistence(
    data_dir: &std::path::Path,
) -> Option<TriggerPersistence> {
    match TriggerPersistence::open(&data_dir.join("triggers.db")) {
        Ok(p) => {
            info!("Supervisor: TriggerPersistence ready");
            Some(p)
        }
        Err(e) => {
            warn!(error = %e, "TriggerPersistence failed to open - trigger history disabled");
            None
        }
    }
}

/// Open the audit trail (`audit.db`). Returns `None` (logged) on failure.
pub(in crate::supervisor) async fn open_audit_trail(
    data_dir: &std::path::Path,
) -> Option<AuditTrailHandle> {
    match AuditTrailHandle::open(&data_dir.join("audit.db")).await {
        Ok(handle) => {
            info!("Supervisor: AuditTrail ready");
            Some(handle)
        }
        Err(e) => {
            warn!(error = %e, "AuditTrail failed to open - audit disabled");
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
    let db_path = data_dir.join("audit_journal.db");
    match load_or_create_journal_key(data_dir) {
        Some(key) => match AuditJournalHandle::open_with_key_bytes(&db_path, key).await {
            Ok(handle) => {
                info!("Supervisor: AuditJournal ready (signed)");
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
            warn!(error = %e, "AuditJournal failed to open - journal disabled");
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
            _ => warn!("audit journal: existing key file is unreadable, regenerating"),
        }
    }

    let mut key = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut key);
    let encoded = base64::engine::general_purpose::STANDARD.encode(key);
    if let Err(e) = write_key_file(&key_path, encoded.as_bytes()) {
        // Sign this session with the in-memory key even if it cannot be
        // persisted (rare: data dir not writable, in which case the db write
        // would also fail).
        warn!(error = %e, "audit journal: could not persist signing key");
        return Some(key.to_vec());
    }
    info!("Supervisor: audit journal signing key generated");
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
    match TaskRepository::open(&data_dir.join("hitl.db")).await {
        Ok(repo) => {
            info!("Supervisor: TaskRepository ready (HITL enabled)");
            Some(Arc::new(repo))
        }
        Err(e) => {
            warn!(error = %e, "TaskRepository failed to open - HITL disabled");
            None
        }
    }
}

/// Open the user memory repository (`user_memory.db`). `None` (logged) on failure.
pub(in crate::supervisor) fn open_user_memory(
    data_dir: &std::path::Path,
) -> Option<std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>> {
    match apollia_memory::user_memory::UserMemoryRepository::new(&data_dir.join("user_memory.db")) {
        Ok(repo) => {
            migrate_legacy_user_profile(data_dir, &repo);
            info!("Supervisor: UserMemoryRepository ready");
            Some(std::sync::Arc::new(std::sync::Mutex::new(repo)))
        }
        Err(e) => {
            warn!(error = %e, "UserMemoryRepository failed to open - user memory disabled");
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
            warn!(error = %e, "legacy __user__.db present but unreadable - skipping profile migration");
            return;
        }
    };
    let entries = match legacy.list_all() {
        Ok(entries) => entries,
        Err(e) => {
            warn!(error = %e, "failed to read legacy user profile - skipping migration");
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
                    warn!(key = %entry.key, error = %e, "failed to migrate a legacy profile entry")
                }
            },
            Err(e) => {
                warn!(key = %entry.key, error = %e, "failed to probe canonical profile entry")
            }
        }
    }
    let retired = legacy_path.with_extension("db.migrated");
    if let Err(e) = std::fs::rename(&legacy_path, &retired) {
        warn!(error = %e, "migrated legacy user profile but could not retire the old db file");
    }
    if migrated > 0 {
        info!(
            count = migrated,
            "migrated legacy user profile entries into user_memory.db"
        );
    }
}

/// Open the plan cache repository (`plan_cache.db`). `None` (logged) on failure.
pub(in crate::supervisor) fn open_plan_cache(
    data_dir: &std::path::Path,
) -> Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>> {
    match apollia_oria::plan_cache::PlanCacheRepository::open(&data_dir.join("plan_cache.db")) {
        Ok(repo) => {
            info!("Supervisor: PlanCacheRepository ready");
            Some(Arc::new(std::sync::Mutex::new(repo)))
        }
        Err(e) => {
            warn!(error = %e, "PlanCacheRepository failed to open - plan caching disabled");
            None
        }
    }
}

/// Open the sidechain repository (`sidechains.db`) and wrap it in a logger.
/// Returns `None` (logged) on failure.
pub(in crate::supervisor) fn open_sidechain_logger(
    data_dir: &std::path::Path,
) -> Option<crate::a2a::SidechainLogger> {
    match crate::a2a::SidechainRepository::open(&data_dir.join("sidechains.db")) {
        Ok(repo) => {
            info!("Supervisor: SidechainRepository ready");
            Some(crate::a2a::SidechainLogger::new(std::sync::Arc::new(
                std::sync::Mutex::new(repo),
            )))
        }
        Err(e) => {
            warn!(error = %e, "SidechainRepository failed to open - sidechain logging disabled");
            None
        }
    }
}

/// Phase 13b: open `projects.db` and seed the built-in project templates.
/// Returns `None` (logged) when the database cannot be opened.
pub(in crate::supervisor) fn open_project_repository(
    data_dir: &std::path::Path,
) -> Option<std::sync::Arc<apollia_tools::ProjectRepository>> {
    let db_path = data_dir.join("projects.db");
    match apollia_tools::ProjectRepository::open(&db_path) {
        Ok(repo) => {
            if let Err(e) = repo.seed_builtin_templates() {
                warn!(error = %e, "ProjectRepository: seed_builtin_templates failed");
            }
            info!("Supervisor: ProjectRepository ready");
            Some(std::sync::Arc::new(repo))
        }
        Err(e) => {
            warn!(error = %e, "ProjectRepository failed to open - projects disabled");
            None
        }
    }
}

/// Resolves `~` at the start of a path to `$HOME`.
pub(crate) fn resolve_home(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(stripped);
        }
    }
    path.to_owned()
}
