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
                warn!(error = %e, "AuditJournal signer failed - opening unsigned");
                open_unsigned_journal(&db_path).await
            }
        },
        None => {
            warn!("audit journal: signing key unavailable - opening unsigned");
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
    if let Err(e) = std::fs::write(&key_path, &encoded) {
        // Sign this session with the in-memory key even if it cannot be
        // persisted (rare: data dir not writable, in which case the db write
        // would also fail).
        warn!(error = %e, "audit journal: could not persist signing key");
        return Some(key.to_vec());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
    }
    info!("Supervisor: audit journal signing key generated");
    Some(key.to_vec())
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
            info!("Supervisor: UserMemoryRepository ready");
            Some(std::sync::Arc::new(std::sync::Mutex::new(repo)))
        }
        Err(e) => {
            warn!(error = %e, "UserMemoryRepository failed to open - user memory disabled");
            None
        }
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
pub(in crate::supervisor) fn resolve_home(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(stripped);
        }
    }
    path.to_owned()
}
