//! `FileWatchTrigger` — source de surveillance du système de fichiers.
//!
//! Spawne une tâche Tokio qui surveille un répertoire via la crate `notify` v6
//! (inotify sur Linux, kqueue sur macOS, ReadDirectoryChanges sur Windows)
//! et forwarde les événements filtrés vers le channel async du `TriggerEngine`.
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
    /// Retourne un `JoinHandle<()>` pour abort lors du hot reload (STORY-073).
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
/// Retourne `None` si l'événement ne correspond pas aux filtres configurés,
/// si le type d'événement est inconnu (`Access`, `Other`, …), ou si le path
/// du fichier est absent de l'événement.
///
/// Cette fonction est pure et testable sans filesystem réel.
pub fn map_notify_event(
    event: notify::Event,
    source: &TriggerSourceConfig,
) -> Option<TriggerPayload> {
    let desired_kinds = match source {
        TriggerSourceConfig::FileWatch { events, .. } => events.as_slice(),
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
    let filename = path.file_name()?.to_string_lossy().into_owned();

    // For Create events: verify the file actually exists as a regular file.
    // This filters spurious Create events emitted by kqueue on macOS when a file is
    // deleted (the backend rescans the directory and may fire a Create for the parent
    // directory or a stale entry). Semantically, a FileWatch Create must point to a
    // real file — if the path is gone or is a directory, skip the event.
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
            pipeline: None,
            enabled: true,
            on_busy: OnBusyPolicy::Queue,
            source: TriggerSourceConfig::FileWatch {
                path: dir.to_path_buf(),
                events,
            },
            input_template: InputTemplate("{{filename}}".into()),
        }
    }

    // ── AC-1 : détection d'un fichier créé ────────────────────────────────

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

    // ── AC-2 : événement Delete ignoré si events = ["create"] ─────────────

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

    // ── AC-3 : map_notify_event — Create matche avec events = ["create"] ──

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

    // ── AC-4 : map_notify_event — Delete filtré si events = ["create"] ────

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
        };

        // WHEN
        let payload = map_notify_event(event, &source);

        // THEN
        assert!(payload.is_none(), "delete should be filtered out");
    }

    // ── AC-5 : map_notify_event — Any matche tous les types ───────────────

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
}
