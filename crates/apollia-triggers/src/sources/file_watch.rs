//! `FileWatchTrigger` — source de surveillance du système de fichiers.
//!
//! Spawne une tâche Tokio qui surveille un répertoire via la crate `notify` v6
//! (inotify sur Linux, kqueue sur macOS, ReadDirectoryChanges sur Windows)
//! et forwarde les événements filtrés vers le channel async du `TriggerEngine`.
//!
//! ## Filtrage des événements
//!
//! Avant propagation, chaque événement est soumis à deux filtres :
//! - **Exclusion de chemin** : tout segment ou pattern correspondant à `exclude_patterns`
//!   est ignoré silencieusement (ex : `.git/`, `node_modules/`, `*.log`).
//! - **Liens symboliques** : si `follow_symlinks = false` (défaut), les chemins
//!   identifiés comme symlinks via `fs::symlink_metadata` sont ignorés.
//!
//! ## Déduplication
//!
//! Les événements répétés sur le même chemin dans une fenêtre de 1 seconde sont
//! dédupliqués : un seul événement est propagé, les doublons sont loggés en
//! `tracing::debug!`.
//!
//! ## Pont sync→async
//!
//! `notify` utilise une API synchrone (`std::sync::mpsc::Sender`). Le pont est
//! réalisé par `recv_timeout(50ms)` dans la boucle de la tâche Tokio : si le
//! channel async est fermé (`tx.is_closed()`), la boucle se termine proprement
//! et le `Watcher` est droppé, ce qui libère les ressources inotify/kqueue.

use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::{
    FileEventKind, TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig,
};

/// Source de déclenchement basée sur la surveillance du système de fichiers.
///
/// Utilise la crate `notify` v6 (cross-platform : inotify/kqueue/FSEvents)
/// avec un pont sync→async via `recv_timeout` pour garantir l'arrêt propre.
pub struct FileWatchTrigger;

impl FileWatchTrigger {
    /// Spawne une tâche Tokio qui surveille le répertoire configuré et forwarde
    /// les événements fichier filtrés vers le channel async du `TriggerEngine`.
    ///
    /// La tâche se termine proprement dès que le channel est fermé (`tx.is_closed()`).
    /// Le `Watcher` est droppé automatiquement, libérant les ressources OS (inotify/kqueue).
    /// Retourne un `JoinHandle<()>` pour abort lors du hot reload.
    pub fn spawn(def: TriggerDefinition, tx: mpsc::Sender<TriggerEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Guard : extraire path depuis la source
            let raw_path = match &def.source {
                TriggerSourceConfig::FileWatch { path, .. } => path.clone(),
                _ => {
                    tracing::error!(
                        trigger = %def.id,
                        "FileWatchTrigger::spawn called with non-FileWatch source"
                    );
                    return;
                }
            };

            let path = expand_tilde(&raw_path);

            // Canal sync notify → thread bloquant
            let (notify_tx, notify_rx) = std::sync::mpsc::channel();

            let mut watcher = match notify::recommended_watcher(notify_tx) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!(
                        trigger = %def.id,
                        error = %e,
                        "failed to create file watcher"
                    );
                    return;
                }
            };

            if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
                tracing::error!(
                    trigger = %def.id,
                    error = %e,
                    path = %path.display(),
                    "failed to watch path"
                );
                return;
            }

            // Pont sync → async : spawn_blocking pour ne pas bloquer les workers Tokio.
            // `blocking_send` est utilisé depuis le contexte bloquant.
            // Le watcher est maintenu en vie dans la closure bloquante.
            let source = def.source.clone();
            let trigger_id = def.id.clone();
            let agent = def.agent.clone();

            tokio::task::spawn_blocking(move || {
                let _watcher = watcher; // maintenu en vie jusqu'à la fin de la closure
                                        // Deduplication: macOS FSEvents emits multiple Create events per `cp`
                                        // (fd allocation + data flush). Suppress repeated fires for the same
                                        // path within a 1-second window.
                let mut dedup: std::collections::HashMap<std::path::PathBuf, std::time::Instant> =
                    std::collections::HashMap::new();
                loop {
                    match notify_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                        Ok(Ok(event)) => {
                            if let Some(payload) = map_notify_event(event, &source) {
                                // Dedup check for File payloads
                                if let TriggerPayload::File {
                                    path: ref file_path,
                                    ..
                                } = payload
                                {
                                    let now = std::time::Instant::now();
                                    if dedup.get(file_path).is_some_and(|last| {
                                        last.elapsed() < std::time::Duration::from_secs(1)
                                    }) {
                                        tracing::debug!(
                                            path = %file_path.display(),
                                            "deduplicated file event"
                                        );
                                        continue;
                                    }
                                    dedup.insert(file_path.clone(), now);
                                    // Keep map bounded — prune entries older than 10s
                                    dedup.retain(|_, t| {
                                        t.elapsed() < std::time::Duration::from_secs(10)
                                    });
                                }
                                let trigger_event = TriggerEvent {
                                    trigger_id: trigger_id.clone(),
                                    agent: agent.clone(),
                                    payload,
                                    fired_at: chrono::Utc::now(),
                                };
                                if tx.blocking_send(trigger_event).is_err() {
                                    // Engine dropped — arrêt propre
                                    break;
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(
                                trigger = %trigger_id,
                                error = %e,
                                "notify error"
                            );
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            // Vérifier si le channel async est fermé (engine arrêté)
                            if tx.is_closed() {
                                break;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .await
            .ok();
        })
    }
}

/// Mappe un événement `notify` vers un [`TriggerPayload::File`] selon les filtres déclarés.
///
/// Retourne `None` si :
/// - L'événement ne correspond pas aux filtres configurés (`events`).
/// - Le type d'événement est inconnu (`Access`, `Other`, …).
/// - Le chemin du fichier est absent de l'événement.
/// - Le chemin correspond à un pattern d'exclusion (`exclude_patterns`).
/// - `follow_symlinks = false` et le chemin est un lien symbolique.
///
/// Cette fonction est pure et testable sans filesystem réel (sauf les vérifications
/// de métadonnées et de symlinks qui requièrent un vrai chemin).
pub fn map_notify_event(
    event: notify::Event,
    source: &TriggerSourceConfig,
) -> Option<TriggerPayload> {
    let (desired_kinds, follow_symlinks, exclude_patterns) = match source {
        TriggerSourceConfig::FileWatch {
            events,
            follow_symlinks,
            exclude_patterns,
            ..
        } => (
            events.as_slice(),
            *follow_symlinks,
            exclude_patterns.as_slice(),
        ),
        _ => return None,
    };

    let file_kind = match event.kind {
        EventKind::Create(_) => FileEventKind::Create,
        EventKind::Modify(_) => FileEventKind::Modify,
        EventKind::Remove(_) => FileEventKind::Delete,
        _ => return None,
    };

    let matches = desired_kinds.contains(&FileEventKind::Any) || desired_kinds.contains(&file_kind);
    if !matches {
        return None;
    }

    let path = event.paths.into_iter().next()?;

    // Exclusion check — avant tout accès filesystem pour minimiser les allocations
    if is_excluded(&path, exclude_patterns) {
        return None;
    }

    // Symlink guard — symlink_metadata ne suit PAS les liens, contrairement à metadata
    if !follow_symlinks {
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() {
                return None;
            }
        }
    }

    let filename = path.file_name()?.to_string_lossy().into_owned();

    // For Create events: verify the file actually exists as a regular file.
    // This filters spurious Create events emitted by kqueue on macOS when a file is
    // deleted (the backend rescans the directory and may fire a Create for the parent
    // directory or a stale entry). Semantically, a FileWatch Create must point to a
    // real file — if the path is gone or is a directory, skip the event.
    // std::fs::metadata follows symlinks intentionally (size of the target, not the link).
    let size_bytes = if file_kind == FileEventKind::Create {
        match std::fs::metadata(&path) {
            Ok(m) if !m.is_dir() => m.len(),
            _ => return None,
        }
    } else {
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    };

    Some(TriggerPayload::File {
        path,
        filename,
        size_bytes,
        event_kind: file_kind,
    })
}

/// Retourne `true` si le chemin correspond à au moins un pattern d'exclusion.
///
/// Patterns supportés :
/// - `"nom"` ou `"nom/"` — correspond si un segment du chemin égale `nom`
/// - `"*.ext"` — correspond si le nom de fichier se termine par `.ext`
fn is_excluded(path: &Path, patterns: &[String]) -> bool {
    patterns.iter().any(|p| matches_exclude_pattern(path, p))
}

/// Teste si le chemin correspond au pattern d'exclusion donné.
fn matches_exclude_pattern(path: &Path, pattern: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    if let Some(ext_suffix) = pattern.strip_prefix("*.") {
        // Extension pattern : *.log, *.tmp
        return path
            .file_name()
            .map(|n| {
                let name = n.to_string_lossy();
                name.ends_with(&format!(".{ext_suffix}"))
            })
            .unwrap_or(false);
    }
    // Segment match : .git, node_modules, __pycache__, .apollia
    path.components()
        .any(|c| c.as_os_str().to_string_lossy() == pattern)
}

/// Résout le tilde `~` vers le répertoire HOME de l'utilisateur.
///
/// Seul `~` en préfixe absolu est résolu (pas `~user`). Si `$HOME` n'est pas
/// défini ou si le path ne commence pas par `~`, il est retourné tel quel.
fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(stripped);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputTemplate, OnBusyPolicy};
    use tempfile::TempDir;

    fn make_file_watch_def(dir: &std::path::Path, events: Vec<FileEventKind>) -> TriggerDefinition {
        TriggerDefinition {
            id: "file-test".into(),
            agent: "file-agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::FileWatch {
                path: dir.to_path_buf(),
                events,
                follow_symlinks: false,
                exclude_patterns: vec![],
            },
            input_template: InputTemplate("{{filename}}".into()),
        }
    }

    // ── détection d'un fichier créé ────────────────────────────────

    #[tokio::test]
    async fn test_ac1_detects_file_creation() {
        // GIVEN
        let dir = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = make_file_watch_def(dir.path(), vec![FileEventKind::Create]);
        let _handle = FileWatchTrigger::spawn(def, tx);
        // Attendre que le watcher soit prêt
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // WHEN
        std::fs::write(dir.path().join("facture.pdf"), b"content").unwrap();

        // THEN
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout waiting for TriggerEvent")
            .expect("channel closed unexpectedly");

        assert!(
            matches!(
                &event.payload,
                TriggerPayload::File { filename, event_kind: FileEventKind::Create, .. }
                if filename == "facture.pdf"
            ),
            "unexpected payload: {:?}",
            event.payload
        );
    }

    // ── événement Delete ignoré si events = ["create"] ─────────────

    #[tokio::test]
    async fn test_ac2_delete_ignored_when_filter_is_create() {
        // GIVEN — watcher démarré sur un répertoire vide
        let dir = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = make_file_watch_def(dir.path(), vec![FileEventKind::Create]);
        let _handle = FileWatchTrigger::spawn(def, tx);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Créer un fichier pour s'assurer que le watcher est actif.
        // Puis drainer tous les événements générés par l'écriture (le backend kqueue
        // peut produire plusieurs events : Create + Modify + watch registration).
        // On attend jusqu'à 300ms de silence pour garantir que le channel est vide.
        let file = dir.path().join("existing.txt");
        std::fs::write(&file, b"data").unwrap();
        loop {
            match tokio::time::timeout(std::time::Duration::from_millis(300), rx.recv()).await {
                Ok(Some(_)) => {} // drainer les événements du write
                _ => break,       // 300ms sans événement = channel stable
            }
        }

        // WHEN — supprimer le fichier (doit être ignoré car filter = ["create"])
        std::fs::remove_file(&file).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // THEN — aucun événement ne doit passer le filtre create-only après un Delete
        assert!(
            rx.try_recv().is_err(),
            "should not receive any event after delete when filter is create-only"
        );
    }

    // ── map_notify_event — Create matche avec events = ["create"] ──

    #[test]
    fn test_map_notify_event_create_matches() {
        // GIVEN — fichier temporaire réel (le check metadata requiert que le path existe)
        use notify::{
            event::{CreateKind, EventAttributes},
            Event, EventKind,
        };
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let tmp_path = tmp.path().to_path_buf();
        let expected_name = tmp_path.file_name().unwrap().to_string_lossy().into_owned();

        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![tmp_path],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/tmp".into(),
            events: vec![FileEventKind::Create],
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN
        assert!(payload.is_some(), "expected Some payload");
        assert!(
            matches!(&payload.unwrap(), TriggerPayload::File { filename, .. }
                if filename == &expected_name),
            "unexpected filename"
        );
    }

    // ── map_notify_event — Delete filtré si events = ["create"] ────

    #[test]
    fn test_map_notify_event_delete_filtered_out() {
        // GIVEN
        use notify::{
            event::{EventAttributes, RemoveKind},
            Event, EventKind,
        };
        let event = Event {
            kind: EventKind::Remove(RemoveKind::File),
            paths: vec!["/tmp/test.txt".into()],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/tmp".into(),
            events: vec![FileEventKind::Create],
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN
        assert!(payload.is_none(), "delete should be filtered out");
    }

    // ── map_notify_event — Any matche tous les types ───────────────

    #[test]
    fn test_map_notify_event_any_matches_all() {
        // GIVEN
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/tmp/test.txt".into()],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/tmp".into(),
            events: vec![FileEventKind::Any],
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN
        assert!(payload.is_some(), "Any filter should match Modify events");
    }

    // ── expand_tilde ───────────────────────────────────────────────────────

    #[test]
    fn test_expand_tilde_resolves_home() {
        // GIVEN — path commençant par ~
        let path: PathBuf = "~/Documents/factures".into();

        // WHEN
        let expanded = expand_tilde(&path);

        // THEN — ne commence plus par ~
        assert!(
            !expanded.starts_with("~"),
            "tilde should have been expanded: {:?}",
            expanded
        );
    }

    #[test]
    fn test_expand_tilde_passthrough_no_tilde() {
        // GIVEN — path absolu sans tilde
        let path: PathBuf = "/tmp/factures".into();

        // WHEN
        let expanded = expand_tilde(&path);

        // THEN — path inchangé
        assert_eq!(expanded, path);
    }

    // ── patterns d'exclusion ──────────────────────────────────

    #[test]
    fn test_default_exclude_patterns_applied() {
        // GIVEN / WHEN
        let patterns = crate::config::default_exclude_patterns();

        // THEN — les 4 patterns attendus sont présents
        assert!(patterns.contains(&".git".to_string()));
        assert!(patterns.contains(&"node_modules".to_string()));
        assert!(patterns.contains(&"__pycache__".to_string()));
        assert!(patterns.contains(&".apollia".to_string()));
        assert_eq!(patterns.len(), 4);
    }

    #[test]
    fn test_git_dir_excluded_by_default() {
        // GIVEN — chemin contenant un segment .git/
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/project/.git/HEAD".into()],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/project".into(),
            events: vec![FileEventKind::Any],
            follow_symlinks: false,
            exclude_patterns: crate::config::default_exclude_patterns(),
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN — .git/ est filtré par les patterns par défaut
        assert!(
            payload.is_none(),
            "events from .git/ must be excluded by default"
        );
    }

    #[test]
    fn test_custom_exclude_pattern_respected() {
        // GIVEN — patterns *.log et tmp/
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/app".into(),
            events: vec![FileEventKind::Any],
            follow_symlinks: false,
            exclude_patterns: vec!["*.log".into(), "tmp".into()],
        };

        // WHEN — fichier .log
        let log_event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/app/app.log".into()],
            attrs: EventAttributes::default(),
        };
        let payload_log = map_notify_event(log_event, &source);

        // WHEN — fichier dans tmp/
        let tmp_event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/app/tmp/data.txt".into()],
            attrs: EventAttributes::default(),
        };
        let payload_tmp = map_notify_event(tmp_event, &source);

        // THEN
        assert!(payload_log.is_none(), "*.log pattern must exclude app.log");
        assert!(
            payload_tmp.is_none(),
            "tmp pattern must exclude tmp/data.txt"
        );
    }

    #[test]
    fn test_empty_exclude_patterns_allows_all() {
        // GIVEN — exclude_patterns vide, chemin .git/
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec!["/project/.git/HEAD".into()],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: "/project".into(),
            events: vec![FileEventKind::Any],
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN — rien n'est filtré si la liste est vide
        assert!(
            payload.is_some(),
            "empty exclude_patterns must not filter any path"
        );
    }

    // ── symlinks ───────────────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn test_symlink_metadata_used_not_metadata() {
        // GIVEN — un vrai lien symbolique
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let dir = TempDir::new().unwrap();
        let real_file = dir.path().join("real.txt");
        std::fs::write(&real_file, b"content").unwrap();
        let symlink_path = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec![symlink_path],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: dir.path().to_path_buf(),
            events: vec![FileEventKind::Any],
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN — symlink_metadata détecte le lien → filtré quand follow_symlinks = false
        assert!(
            payload.is_none(),
            "symlink must be filtered when follow_symlinks = false"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_outside_watch_dir_no_event() {
        // GIVEN — symlink dans le répertoire surveillé pointant vers un fichier extérieur
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let watch_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();
        let target_file = target_dir.path().join("external.txt");
        std::fs::write(&target_file, b"external content").unwrap();
        let symlink_in_watch = watch_dir.path().join("link_to_external.txt");
        std::os::unix::fs::symlink(&target_file, &symlink_in_watch).unwrap();

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec![symlink_in_watch],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: watch_dir.path().to_path_buf(),
            events: vec![FileEventKind::Any],
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN — symlink pointant hors périmètre filtré quand follow_symlinks = false
        assert!(
            payload.is_none(),
            "symlink pointing outside watch dir must be excluded when follow_symlinks = false"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_follow_symlinks_true_generates_events() {
        // GIVEN — lien symbolique avec follow_symlinks = true
        use notify::{
            event::{DataChange, EventAttributes, ModifyKind},
            Event, EventKind,
        };
        let dir = TempDir::new().unwrap();
        let real_file = dir.path().join("real.txt");
        std::fs::write(&real_file, b"content").unwrap();
        let symlink_path = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec![symlink_path],
            attrs: EventAttributes::default(),
        };
        let source = TriggerSourceConfig::FileWatch {
            path: dir.path().to_path_buf(),
            events: vec![FileEventKind::Any],
            follow_symlinks: true,
            exclude_patterns: vec![],
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN — événement propagé quand follow_symlinks = true
        assert!(
            payload.is_some(),
            "symlink event must pass through when follow_symlinks = true"
        );
    }

    // ── déduplication loggée en debug ──────────────────────────

    #[tokio::test]
    async fn test_deduplicated_events_logged_debug() {
        // GIVEN — watcher actif sur un répertoire
        let dir = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let def = TriggerDefinition {
            id: "dedup-test".into(),
            agent: "dedup-agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::FileWatch {
                path: dir.path().to_path_buf(),
                events: vec![FileEventKind::Any],
                follow_symlinks: false,
                exclude_patterns: vec![],
            },
            input_template: InputTemplate("{{filename}}".into()),
        };
        let _handle = FileWatchTrigger::spawn(def, tx);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // WHEN — créer puis réécrire rapidement le même fichier (< 1s entre les deux)
        let file = dir.path().join("report.pdf");
        std::fs::write(&file, b"first write").unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        std::fs::write(&file, b"second write").unwrap();

        // Attendre la fenêtre de déduplication (1s) + marge
        tokio::time::sleep(std::time::Duration::from_millis(1300)).await;

        // THEN — au plus 1 événement propagé dans la fenêtre de déduplication
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert!(
            count <= 1,
            "deduplicated writes must result in at most 1 event within the debounce window, got {count}"
        );
    }
}
