//! Téléchargement de modèles GGUF depuis HuggingFace avec suivi de progression.
//!
//! - Streaming via `reqwest` avec header `Range` pour la reprise.
//! - Un [`DownloadManager`] gère un pool de téléchargements concurrents.
//! - La progression est reportée via [`DownloadProgress`] — à router vers
//!   les events Tauri côté desktop.

#![cfg(feature = "cloud")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Erreurs du module de téléchargement.
#[derive(Debug, Error)]
pub enum DownloadError {
    /// Erreur réseau.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// Erreur I/O (écriture fichier).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Téléchargement annulé par l'utilisateur.
    #[error("download cancelled")]
    Cancelled,
    /// Téléchargement introuvable.
    #[error("download '{0}' not found")]
    NotFound(String),
}

// ─────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────

/// Identifiant unique d'un téléchargement.
pub type DownloadId = String;

/// Progression d'un téléchargement en cours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Identifiant du téléchargement.
    pub id: DownloadId,
    /// Octets téléchargés jusqu'à présent.
    pub downloaded_bytes: u64,
    /// Taille totale attendue (si connue via `Content-Length`).
    pub total_bytes: Option<u64>,
    /// Vitesse instantanée en bytes/seconde.
    pub speed_bps: f64,
    /// Chemin de destination.
    pub dest_path: PathBuf,
    /// Statut courant.
    pub status: DownloadStatus,
}

/// Statut d'un téléchargement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    /// En cours de téléchargement.
    InProgress,
    /// Terminé avec succès.
    Completed,
    /// Annulé par l'utilisateur.
    Cancelled,
    /// Erreur irrécupérable.
    Failed,
}

/// Requête de démarrage d'un téléchargement.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// URL du fichier à télécharger.
    pub url: String,
    /// Répertoire de destination (ex. `~/.apollia/models/`).
    pub dest_dir: PathBuf,
    /// Nom de fichier de destination (optionnel — déduit de l'URL si absent).
    pub filename: Option<String>,
    /// Token HF pour les modèles gated (optionnel).
    pub hf_token: Option<String>,
}

// ─────────────────────────────────────────────
// Manager
// ─────────────────────────────────────────────

type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync + 'static>;

/// Gestionnaire de téléchargements concurrents.
///
/// Chaque téléchargement reçoit un UUID et peut être annulé individuellement.
/// La progression est reportée via un callback injectable (Tauri events côté desktop).
pub struct DownloadManager {
    client: reqwest::Client,
    /// Map download_id → CancellationToken pour les annulations.
    active: Arc<Mutex<HashMap<DownloadId, CancellationToken>>>,
}

impl DownloadManager {
    /// Crée un nouveau gestionnaire.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Apollia-OS/1.0")
            // connect_timeout only — no total-request timeout so large model downloads
            // (multi-GB) are not killed mid-stream after 60 s.
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client build never fails");
        Self {
            client,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Démarre un téléchargement en arrière-plan.
    ///
    /// Retourne le `DownloadId` immédiatement.
    /// La progression est reportée via `on_progress` à chaque chunk reçu.
    pub fn start(&self, request: DownloadRequest, on_progress: ProgressCallback) -> DownloadId {
        let id = Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();

        {
            let mut active = self.active.lock().expect("active lock not poisoned");
            active.insert(id.clone(), cancel.clone());
        }

        let client = self.client.clone();
        let active = self.active.clone();
        let task_id = id.clone();

        tokio::spawn(async move {
            let result = run_download(
                client,
                request,
                task_id.clone(),
                cancel,
                on_progress.clone(),
            )
            .await;

            // Clean up active map
            {
                let mut map = active.lock().expect("active lock");
                map.remove(&task_id);
            }

            if let Err(e) = result {
                tracing::error!(id = %task_id, error = %e, "download failed");
            }
        });

        id
    }

    /// Annule un téléchargement en cours.
    ///
    /// # Errors
    /// [`DownloadError::NotFound`] si l'ID est inconnu ou déjà terminé.
    pub fn cancel(&self, id: &str) -> Result<(), DownloadError> {
        let active = self.active.lock().expect("active lock");
        match active.get(id) {
            Some(token) => {
                token.cancel();
                Ok(())
            }
            None => Err(DownloadError::NotFound(id.to_string())),
        }
    }

    /// Retourne la liste des IDs de téléchargements actifs.
    pub fn active_ids(&self) -> Vec<DownloadId> {
        self.active
            .lock()
            .expect("active lock")
            .keys()
            .cloned()
            .collect()
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────
// Core download logic
// ─────────────────────────────────────────────

async fn run_download(
    client: reqwest::Client,
    request: DownloadRequest,
    id: DownloadId,
    cancel: CancellationToken,
    on_progress: ProgressCallback,
) -> Result<(), DownloadError> {
    let filename = request.filename.unwrap_or_else(|| {
        request
            .url
            .rsplit('/')
            .next()
            .unwrap_or("model.gguf")
            .to_string()
    });

    let dest_path = request.dest_dir.join(&filename);

    // Check for partial download to resume.
    let already_downloaded = if dest_path.exists() {
        tokio::fs::metadata(&dest_path).await?.len()
    } else {
        0
    };

    let mut req = client.get(&request.url);
    if let Some(token) = &request.hf_token {
        req = req.header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if already_downloaded > 0 {
        req = req.header(
            reqwest::header::RANGE,
            format!("bytes={already_downloaded}-"),
        );
    }

    let resp = req.send().await?;
    let total_bytes = resp.content_length().map(|l| l + already_downloaded);

    let mut file = OpenOptions::new()
        .create(true)
        .append(already_downloaded > 0)
        .write(true)
        .open(&dest_path)
        .await?;

    let mut stream = resp.bytes_stream();
    let mut downloaded = already_downloaded;
    let start = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                drop(file);
                if let Err(e) = tokio::fs::remove_file(&dest_path).await {
                    tracing::warn!(
                        id = %id,
                        path = %dest_path.display(),
                        error = %e,
                        "failed to remove partial file after cancel"
                    );
                }
                on_progress(DownloadProgress {
                    id: id.clone(),
                    downloaded_bytes: 0,
                    total_bytes,
                    speed_bps: 0.0,
                    dest_path: dest_path.clone(),
                    status: DownloadStatus::Cancelled,
                });
                return Err(DownloadError::Cancelled);
            }
            chunk = stream.next() => {
                match chunk {
                    None => break,
                    Some(Err(e)) => return Err(DownloadError::Http(e)),
                    Some(Ok(bytes)) => {
                        file.write_all(&bytes).await?;
                        downloaded += bytes.len() as u64;
                        let elapsed = start.elapsed().as_secs_f64().max(0.001);
                        let speed_bps = downloaded as f64 / elapsed;
                        on_progress(DownloadProgress {
                            id: id.clone(),
                            downloaded_bytes: downloaded,
                            total_bytes,
                            speed_bps,
                            dest_path: dest_path.clone(),
                            status: DownloadStatus::InProgress,
                        });
                    }
                }
            }
        }
    }

    file.flush().await?;

    on_progress(DownloadProgress {
        id: id.clone(),
        downloaded_bytes: downloaded,
        total_bytes: Some(downloaded),
        speed_bps: 0.0,
        dest_path: dest_path.clone(),
        status: DownloadStatus::Completed,
    });

    tracing::info!(
        id = %id,
        path = %dest_path.display(),
        bytes = downloaded,
        "model download completed"
    );

    Ok(())
}
